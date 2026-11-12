//! Local terminal sessions (architecture §10, task 4A.2): a shell on a pseudo terminal
//! (ConPTY on Windows, `openpty` elsewhere) streamed to `kb://term` pages.
//!
//! Security: a local shell runs with the user's full rights, so only `kb://` pages (the bridge
//! is closed to web content) can open one, and a session dies with the query that opened it.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};

type Emit = Arc<dyn Fn(Output) + Send + Sync>;

/// What a session produces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Output {
    Data(Vec<u8>),
    Exited(Option<u32>),
}

enum Session {
    Local {
        master: Box<dyn MasterPty + Send>,
        writer: Box<dyn Write + Send>,
        killer: Box<dyn portable_pty::ChildKiller + Send + Sync>,
    },
    Ssh(kb_ssh::Shell),
}

static SESSIONS: OnceLock<Mutex<HashMap<u32, Session>>> = OnceLock::new();
static NEXT: AtomicU32 = AtomicU32::new(1);

fn sessions() -> &'static Mutex<HashMap<u32, Session>> {
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The user's shell: `%COMSPEC%`-style PowerShell on Windows, `$SHELL` elsewhere.
pub fn default_shell() -> (String, Vec<String>) {
    if cfg!(windows) {
        for candidate in ["pwsh.exe", "powershell.exe"] {
            if which(candidate) {
                return (candidate.into(), vec!["-NoLogo".into()]);
            }
        }
        (
            std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".into()),
            Vec::new(),
        )
    } else {
        (
            std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into()),
            Vec::new(),
        )
    }
}

fn which(exe: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(exe).is_file()))
}

/// Starts a shell session and returns its id. `emit` receives output on a reader thread.
pub fn open(cols: u16, rows: u16, emit: Emit) -> Result<u32, String> {
    let pty = native_pty_system()
        .openpty(PtySize {
            rows: rows.max(2),
            cols: cols.max(2),
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| e.to_string())?;
    let (program, args) = default_shell();
    let mut cmd = CommandBuilder::new(program);
    cmd.args(args);
    if let Some(home) = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }) {
        cmd.cwd(home);
    }
    cmd.env("TERM", "xterm-256color");
    let mut child = pty.slave.spawn_command(cmd).map_err(|e| e.to_string())?;
    let killer = child.clone_killer();
    let mut reader = pty.master.try_clone_reader().map_err(|e| e.to_string())?;
    let writer = pty.master.take_writer().map_err(|e| e.to_string())?;
    drop(pty.slave);

    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    sessions().lock().map_err(|_| "poisoned")?.insert(
        id,
        Session::Local {
            master: pty.master,
            writer,
            killer,
        },
    );
    // Reader: forwards output until the pseudo terminal closes.
    std::thread::Builder::new()
        .name(format!("kb-term-{id}-out"))
        .spawn({
            let emit = emit.clone();
            move || {
                let mut buf = [0u8; 8192];
                while let Ok(n) = reader.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    emit(Output::Data(buf[..n].to_vec()));
                }
            }
        })
        .map_err(|e| e.to_string())?;
    // Waiter: when the shell exits, drop the session (closing the master unblocks the reader on
    // ConPTY, which does not report EOF by itself) and announce the exit.
    std::thread::Builder::new()
        .name(format!("kb-term-{id}-wait"))
        .spawn(move || {
            let code = child.wait().ok().map(|s| s.exit_code());
            if let Ok(mut s) = sessions().lock() {
                s.remove(&id);
            }
            emit(Output::Exited(code));
        })
        .map_err(|e| e.to_string())?;
    Ok(id)
}

/// Connection target for an SSH session (credentials already resolved by the caller).
pub struct SshTarget {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub auth: kb_ssh::Auth,
    pub known_hosts: std::path::PathBuf,
    pub trust_new_host: bool,
}

/// Opens an interactive SSH shell. Errors for unknown hosts start with `unknown-host:<fingerprint>`
/// and for changed keys with `host-key-changed:`, so the page can ask the user what to do.
pub fn open_ssh(cols: u16, rows: u16, target: SshTarget, emit: Emit) -> Result<u32, String> {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let cfg = kb_ssh::Config {
        host: target.host,
        port: target.port,
        user: target.user,
        auth: target.auth,
        known_hosts: target.known_hosts,
        trust_new_host: target.trust_new_host,
        cols: cols.max(2),
        rows: rows.max(2),
    };
    let shell = kb_ssh::open_shell(
        cfg,
        Arc::new(move |e| match e {
            kb_ssh::Event::Data(d) => emit(Output::Data(d)),
            kb_ssh::Event::Exited(c) => {
                if let Ok(mut s) = sessions().lock() {
                    s.remove(&id);
                }
                emit(Output::Exited(c));
            }
        }),
    )
    .map_err(|e| match e {
        kb_ssh::SshError::UnknownHost { fingerprint } => format!("unknown-host:{fingerprint}"),
        kb_ssh::SshError::HostKeyChanged { expected, got } => {
            format!("host-key-changed:{expected} -> {got}")
        }
        other => other.to_string(),
    })?;
    sessions()
        .lock()
        .map_err(|_| "poisoned")?
        .insert(id, Session::Ssh(shell));
    Ok(id)
}

/// Full `kb-ssh` configuration for a page request (terminal size is irrelevant for SFTP/tunnels).
pub fn ssh_config(req: &crate::protocol::SshRequest) -> Result<kb_ssh::Config, String> {
    let t = resolve_ssh(req)?;
    Ok(kb_ssh::Config {
        host: t.host,
        port: t.port,
        user: t.user,
        auth: t.auth,
        known_hosts: t.known_hosts,
        trust_new_host: t.trust_new_host,
        cols: 80,
        rows: 24,
    })
}

/// Error text for the pages: `unknown-host:<fp>` and `host-key-changed:<a> -> <b>` are parsed there.
pub fn ssh_err(e: kb_ssh::SshError) -> String {
    match e {
        kb_ssh::SshError::UnknownHost { fingerprint } => format!("unknown-host:{fingerprint}"),
        kb_ssh::SshError::HostKeyChanged { expected, got } => {
            format!("host-key-changed:{expected} -> {got}")
        }
        other => other.to_string(),
    }
}

/// Turns a page request into a connection target: credentials from the unlocked vault or the
/// one-off password. Key entries keep the PEM in `password` and the passphrase in `notes`.
pub fn resolve_ssh(req: &crate::protocol::SshRequest) -> Result<SshTarget, String> {
    let known_hosts = crate::shell::profile_dir()
        .ok_or("no profile directory")?
        .join("known_hosts");
    let (user, auth) = if let Some(id) = &req.vault_id {
        let (kind, secret) = crate::vault::secret(id)?;
        let user = if req.user.is_empty() {
            secret.username.clone()
        } else {
            req.user.clone()
        };
        let auth = match kind {
            kb_vault::Kind::SshKey => kb_ssh::Auth::Key {
                pem: secret.password.clone(),
                passphrase: Some(secret.notes.clone()).filter(|p| !p.is_empty()),
            },
            _ => kb_ssh::Auth::Password(secret.password.clone()),
        };
        (user, auth)
    } else {
        (
            req.user.clone(),
            kb_ssh::Auth::Password(
                req.password
                    .clone()
                    .ok_or("password or vault entry required")?,
            ),
        )
    };
    if user.is_empty() {
        return Err("user name required".into());
    }
    Ok(SshTarget {
        host: req.host.trim().to_string(),
        port: req.port,
        user,
        auth,
        known_hosts,
        trust_new_host: req.trust_new_host,
    })
}

pub fn write(id: u32, data: &[u8]) -> Result<(), String> {
    let mut s = sessions().lock().map_err(|_| "poisoned")?;
    match s.get_mut(&id).ok_or("no such session")? {
        Session::Local { writer, .. } => {
            writer.write_all(data).map_err(|e| e.to_string())?;
            writer.flush().map_err(|e| e.to_string())
        }
        Session::Ssh(shell) => {
            shell.write(data);
            Ok(())
        }
    }
}

pub fn resize(id: u32, cols: u16, rows: u16) -> Result<(), String> {
    let s = sessions().lock().map_err(|_| "poisoned")?;
    match s.get(&id).ok_or("no such session")? {
        Session::Local { master, .. } => master
            .resize(PtySize {
                rows: rows.max(2),
                cols: cols.max(2),
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| e.to_string()),
        Session::Ssh(shell) => {
            shell.resize(cols.max(2), rows.max(2));
            Ok(())
        }
    }
}

/// Ends the session; local shells are killed (the waiter then reports `Exited`), SSH channels
/// are closed.
pub fn close(id: u32) {
    if let Ok(mut s) = sessions().lock()
        && let Some(session) = s.remove(&id)
    {
        match session {
            Session::Local { mut killer, .. } => {
                let _ = killer.kill();
            }
            Session::Ssh(shell) => shell.close(),
        }
    }
}

pub fn count() -> usize {
    sessions().lock().map_or(0, |s| s.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;
    use std::time::Duration;

    #[test]
    fn runs_a_command_through_the_pty() {
        let (tx, rx) = channel();
        let tx = Mutex::new(tx);
        let id = open(
            100,
            30,
            Arc::new(move |o| {
                let _ = tx.lock().unwrap().send(o);
            }),
        )
        .unwrap();
        write(id, b"echo kb-terminal-ok\r\n").unwrap();
        let mut seen = String::new();
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        while std::time::Instant::now() < deadline && !seen.contains("kb-terminal-ok") {
            if let Ok(Output::Data(d)) = rx.recv_timeout(Duration::from_millis(500)) {
                let text = String::from_utf8_lossy(&d).to_string();
                // ConPTY asks for the cursor position at start-up and waits for the answer, as
                // a real terminal emulator (xterm.js) would give it.
                if text.contains("[6n") {
                    write(id, b"[1;1R").unwrap();
                }
                seen.push_str(&text);
            }
        }
        assert!(seen.contains("kb-terminal-ok"), "no output, got: {seen:?}");
        resize(id, 80, 24).unwrap();
        write(id, b"exit\r\n").unwrap();
        let mut exited = false;
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        while std::time::Instant::now() < deadline && !exited {
            exited = matches!(
                rx.recv_timeout(Duration::from_millis(500)),
                Ok(Output::Exited(_))
            );
        }
        assert!(exited, "shell did not exit");
        assert_eq!(count(), 0);
        assert!(write(id, b"x").is_err());
    }
}
