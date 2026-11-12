//! Tests run against an in-process russh server: password `pw` for user `tester`.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use russh::keys::{Algorithm, PrivateKey};
use russh::server::{self, Auth as SAuth, Msg, Server as _, Session};
use russh::{Channel, ChannelId};
use tokio::net::TcpListener;

use super::*;

#[derive(Clone)]
struct TestServer;

impl server::Server for TestServer {
    type Handler = Self;
    fn new_client(&mut self, _: Option<std::net::SocketAddr>) -> Self {
        Self
    }
}

impl server::Handler for TestServer {
    type Error = russh::Error;

    async fn auth_password(&mut self, user: &str, password: &str) -> Result<SAuth, Self::Error> {
        if user == "tester" && password == "pw" {
            Ok(SAuth::Accept)
        } else {
            Ok(SAuth::reject())
        }
    }

    async fn channel_open_session(
        &mut self,
        _channel: Channel<Msg>,
        reply: server::ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        reply.accept().await;
        Ok(())
    }

    async fn pty_request(
        &mut self,
        channel: ChannelId,
        _term: &str,
        _c: u32,
        _r: u32,
        _pw: u32,
        _ph: u32,
        _modes: &[(russh::Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        Ok(())
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        session.data(channel, b"welcome\r\n".to_vec())?;
        Ok(())
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if data == b"exit\r" {
            session.exit_status_request(channel, 7)?;
            session.close(channel)?;
        } else {
            let mut echo = b"echo:".to_vec();
            echo.extend_from_slice(data);
            session.data(channel, echo)?;
        }
        Ok(())
    }
}

/// Starts the server on a random port; returns the port and the host key fingerprint.
fn start_server() -> (u16, String) {
    let key = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).unwrap();
    let fingerprint = key.public_key().fingerprint(HashAlg::Sha256).to_string();
    let config = Arc::new(server::Config {
        keys: vec![key],
        auth_rejection_time: Duration::from_millis(1),
        auth_rejection_time_initial: Some(Duration::from_millis(0)),
        ..Default::default()
    });
    let listener = runtime()
        .block_on(TcpListener::bind(("127.0.0.1", 0)))
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    runtime().spawn(async move {
        let mut s = TestServer;
        let _ = s.run_on_socket(config, &listener).await;
    });
    (port, fingerprint)
}

fn cfg(port: u16, dir: &Path, pw: &str, trust: bool) -> Config {
    Config {
        host: "127.0.0.1".into(),
        port,
        user: "tester".into(),
        auth: Auth::Password(pw.into()),
        known_hosts: dir.join("known_hosts"),
        trust_new_host: trust,
        cols: 80,
        rows: 24,
    }
}

type Log = Arc<Mutex<Vec<Event>>>;

fn collect() -> (Arc<dyn Fn(Event) + Send + Sync>, Log) {
    let log = Arc::new(Mutex::new(Vec::new()));
    let l = log.clone();
    (Arc::new(move |e| l.lock().unwrap().push(e)), log)
}

fn wait_for(log: &Log, f: impl Fn(&[Event]) -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(10) {
        if f(&log.lock().unwrap()) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

fn text(events: &[Event]) -> String {
    let bytes: Vec<u8> = events
        .iter()
        .flat_map(|e| match e {
            Event::Data(d) => d.clone(),
            Event::Exited(_) => Vec::new(),
        })
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("kb-ssh-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn unknown_host_is_refused_until_trusted_then_remembered() {
    let (port, fp) = start_server();
    let dir = tmp("tofu");
    let (cb, _) = collect();
    let err = open_shell(cfg(port, &dir, "pw", false), cb).err().unwrap();
    assert_eq!(
        err,
        SshError::UnknownHost {
            fingerprint: fp.clone()
        }
    );

    let (cb, log) = collect();
    let shell = open_shell(cfg(port, &dir, "pw", true), cb).unwrap();
    assert!(wait_for(&log, |e| text(e).contains("welcome")));
    drop(shell);
    let known = std::fs::read_to_string(dir.join("known_hosts")).unwrap();
    assert!(known.contains(&fp));

    // Now known: connects without the trust flag.
    let (cb, log) = collect();
    let _shell = open_shell(cfg(port, &dir, "pw", false), cb).unwrap();
    assert!(wait_for(&log, |e| text(e).contains("welcome")));
}

#[test]
fn changed_host_key_is_always_refused() {
    let (port, _fp) = start_server();
    let dir = tmp("changed");
    std::fs::write(
        dir.join("known_hosts"),
        format!("127.0.0.1:{port} SHA256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\n"),
    )
    .unwrap();
    let (cb, _) = collect();
    let err = open_shell(cfg(port, &dir, "pw", true), cb).err().unwrap();
    assert!(matches!(err, SshError::HostKeyChanged { .. }), "{err:?}");
}

#[test]
fn wrong_password_fails() {
    let (port, _fp) = start_server();
    let dir = tmp("auth");
    let (cb, _) = collect();
    let err = open_shell(cfg(port, &dir, "nope", true), cb).err().unwrap();
    assert_eq!(err, SshError::AuthFailed);
}

#[test]
fn shell_echoes_input_and_reports_exit() {
    let (port, _fp) = start_server();
    let dir = tmp("echo");
    let (cb, log) = collect();
    let shell = open_shell(cfg(port, &dir, "pw", true), cb).unwrap();
    assert!(wait_for(&log, |e| text(e).contains("welcome")));
    shell.write(b"hi");
    assert!(wait_for(&log, |e| text(e).contains("echo:hi")));
    shell.resize(100, 30);
    shell.write(b"exit\r");
    assert!(wait_for(&log, |e| e
        .iter()
        .any(|x| matches!(x, Event::Exited(Some(7))))));
}
