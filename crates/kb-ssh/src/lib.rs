//! SSH client sessions for the terminal (task 4A.3, architecture §10.3).
//!
//! - Password and private-key authentication (key text comes from the vault, never from disk
//!   unless the caller reads a file itself).
//! - Host keys are verified against a `known_hosts` file of our own format
//!   (`host:port SHA256:<fingerprint>`); unknown hosts are refused until the caller
//!   explicitly trusts them, changed keys are always refused.
//! - A private tokio runtime keeps the API synchronous for the shell.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use russh::client;
use russh::keys::{HashAlg, PrivateKeyWithHashAlg, decode_secret_key};
use russh::{ChannelMsg, Disconnect};
use tokio::runtime::Runtime;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub enum Auth {
    Password(String),
    /// OpenSSH/PEM private key text and its optional passphrase.
    Key {
        pem: String,
        passphrase: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub auth: Auth,
    pub known_hosts: PathBuf,
    /// Accept and remember a host key we have never seen (the UI asks the user first).
    pub trust_new_host: bool,
    pub cols: u16,
    pub rows: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SshError {
    /// Never seen this host; carries the fingerprint to show to the user.
    UnknownHost {
        fingerprint: String,
    },
    /// The host presented a different key than the one on record.
    HostKeyChanged {
        expected: String,
        got: String,
    },
    AuthFailed,
    Other(String),
}

impl std::fmt::Display for SshError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownHost { fingerprint } => write!(f, "unknown host key {fingerprint}"),
            Self::HostKeyChanged { expected, got } => write!(
                f,
                "HOST KEY CHANGED (possible attack): expected {expected}, got {got}"
            ),
            Self::AuthFailed => write!(f, "authentication failed"),
            Self::Other(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for SshError {}

impl From<russh::Error> for SshError {
    fn from(e: russh::Error) -> Self {
        Self::Other(e.to_string())
    }
}

/// What the remote end produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Data(Vec<u8>),
    Exited(Option<u32>),
}

enum Input {
    Data(Vec<u8>),
    Resize(u16, u16),
    Close,
}

pub struct Shell {
    tx: mpsc::UnboundedSender<Input>,
}

impl Shell {
    pub fn write(&self, data: &[u8]) {
        let _ = self.tx.send(Input::Data(data.to_vec()));
    }

    pub fn resize(&self, cols: u16, rows: u16) {
        let _ = self.tx.send(Input::Resize(cols, rows));
    }

    pub fn close(&self) {
        let _ = self.tx.send(Input::Close);
    }
}

impl Drop for Shell {
    fn drop(&mut self) {
        self.close();
    }
}

fn runtime() -> &'static Runtime {
    static RT: OnceLock<Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("kb-ssh")
            .enable_all()
            .build()
            .expect("tokio runtime")
    })
}

// ------------------------------------------------------------------ known_hosts

fn host_id(host: &str, port: u16) -> String {
    format!("{}:{port}", host.to_ascii_lowercase())
}

fn lookup(path: &Path, id: &str) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    text.lines().find_map(|l| {
        let (h, fp) = l.split_once(' ')?;
        (h == id).then(|| fp.trim().to_string())
    })
}

fn remember(path: &Path, id: &str, fingerprint: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(f, "{id} {fingerprint}")
}

pub(crate) struct Client {
    known_hosts: PathBuf,
    id: String,
    trust_new: bool,
    verdict: Arc<Mutex<Option<SshError>>>,
}

impl client::Handler for Client {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        key: &russh::keys::PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let fingerprint = match key {
            russh::keys::PublicKeyOrCertificate::PublicKey { key, .. } => {
                key.fingerprint(HashAlg::Sha256).to_string()
            }
            // Host certificates are not supported yet: refuse rather than guess.
            _ => return Ok(false),
        };
        let set = |e: SshError| {
            if let Ok(mut v) = self.verdict.lock() {
                *v = Some(e);
            }
        };
        match lookup(&self.known_hosts, &self.id) {
            Some(known) if known == fingerprint => Ok(true),
            Some(known) => {
                set(SshError::HostKeyChanged {
                    expected: known,
                    got: fingerprint,
                });
                Ok(false)
            }
            None if self.trust_new => {
                let _ = remember(&self.known_hosts, &self.id, &fingerprint);
                Ok(true)
            }
            None => {
                set(SshError::UnknownHost { fingerprint });
                Ok(false)
            }
        }
    }
}

// ---------------------------------------------------------------------- connect

/// Connects and authenticates (host key check included). Runs on the private runtime.
pub(crate) async fn connect_authed(cfg: &Config) -> Result<client::Handle<Client>, SshError> {
    let verdict = Arc::new(Mutex::new(None));
    let handler = Client {
        known_hosts: cfg.known_hosts.clone(),
        id: host_id(&cfg.host, cfg.port),
        trust_new: cfg.trust_new_host,
        verdict: verdict.clone(),
    };
    let config = Arc::new(client::Config {
        inactivity_timeout: None,
        keepalive_interval: Some(Duration::from_secs(30)),
        ..Default::default()
    });
    let connect = tokio::time::timeout(
        Duration::from_secs(15),
        client::connect(config, (cfg.host.as_str(), cfg.port), handler),
    )
    .await
    .map_err(|_| SshError::Other("connection timed out".into()))?;
    let mut handle = match connect {
        Ok(h) => h,
        Err(e) => {
            // A refused host key surfaces as a generic error; report the real reason.
            if let Some(v) = verdict.lock().ok().and_then(|mut v| v.take()) {
                return Err(v);
            }
            return Err(e.into());
        }
    };
    let ok = match &cfg.auth {
        Auth::Password(pw) => handle
            .authenticate_password(cfg.user.clone(), pw.clone())
            .await?
            .success(),
        Auth::Key { pem, passphrase } => {
            let key = decode_secret_key(pem, passphrase.as_deref())
                .map_err(|e| SshError::Other(format!("cannot read private key: {e}")))?;
            let hash = handle.best_supported_rsa_hash().await?.flatten();
            handle
                .authenticate_publickey(
                    cfg.user.clone(),
                    PrivateKeyWithHashAlg::new(Arc::new(key), hash),
                )
                .await?
                .success()
        }
    };
    if !ok {
        return Err(SshError::AuthFailed);
    }
    Ok(handle)
}

/// Connects, authenticates and starts an interactive shell. `on_event` is called from the SSH
/// worker thread for every chunk of output and once at the end.
pub fn open_shell(
    cfg: Config,
    on_event: Arc<dyn Fn(Event) + Send + Sync>,
) -> Result<Shell, SshError> {
    runtime().block_on(async move {
        let handle = connect_authed(&cfg).await?;
        let mut channel = handle.channel_open_session().await?;
        channel
            .request_pty(
                false,
                "xterm-256color",
                u32::from(cfg.cols),
                u32::from(cfg.rows),
                0,
                0,
                &[],
            )
            .await?;
        channel.request_shell(false).await?;

        let (tx, mut rx) = mpsc::unbounded_channel::<Input>();
        tokio::spawn(async move {
            let mut code = None;
            loop {
                tokio::select! {
                    input = rx.recv() => match input {
                        Some(Input::Data(d)) => {
                            if channel.data_bytes(d).await.is_err() {
                                break;
                            }
                        }
                        Some(Input::Resize(c, r)) => {
                            let _ = channel.window_change(u32::from(c), u32::from(r), 0, 0).await;
                        }
                        Some(Input::Close) | None => break,
                    },
                    msg = channel.wait() => match msg {
                        Some(ChannelMsg::Data { data }) => on_event(Event::Data(data.to_vec())),
                        Some(ChannelMsg::ExtendedData { data, .. }) => {
                            on_event(Event::Data(data.to_vec()));
                        }
                        Some(ChannelMsg::ExitStatus { exit_status }) => code = Some(exit_status),
                        Some(ChannelMsg::Eof | ChannelMsg::Close) | None => break,
                        _ => {}
                    },
                }
            }
            let _ = handle.disconnect(Disconnect::ByApplication, "", "en").await;
            on_event(Event::Exited(code));
        });
        Ok(Shell { tx })
    })
}

pub mod ftp;
pub mod sftp;
pub mod telnet;
pub mod tunnel;

#[cfg(test)]
mod tests;
