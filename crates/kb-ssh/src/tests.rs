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

// ------------------------------------------------------------------------ SFTP

mod sftp_tests {
    use std::collections::{BTreeMap, BTreeSet, HashMap};

    use russh_sftp::protocol::{
        Attrs, Data, File, FileAttributes, Handle, Name, OpenFlags, Status, StatusCode, Version,
    };

    use super::*;
    use crate::sftp::Sftp;

    fn ok(id: u32) -> Status {
        Status {
            id,
            status_code: StatusCode::Ok,
            error_message: "Ok".into(),
            language_tag: "en-US".into(),
        }
    }

    /// In-memory file system: flat paths under `/`, directories tracked separately.
    #[derive(Default)]
    struct Fs {
        files: BTreeMap<String, Vec<u8>>,
        dirs: BTreeSet<String>,
        listed: BTreeSet<String>,
    }

    fn base(path: &str) -> String {
        path.trim_start_matches("./")
            .trim_start_matches('/')
            .to_string()
    }

    fn attrs(size: u64, dir: bool) -> FileAttributes {
        FileAttributes {
            size: Some(size),
            permissions: Some(if dir { 0o040755 } else { 0o100644 }),
            ..Default::default()
        }
    }

    impl russh_sftp::server::Handler for Fs {
        type Error = StatusCode;

        fn unimplemented(&self) -> Self::Error {
            StatusCode::OpUnsupported
        }

        async fn init(
            &mut self,
            _v: u32,
            _e: HashMap<String, String>,
        ) -> Result<Version, Self::Error> {
            Ok(Version::new())
        }

        async fn realpath(&mut self, id: u32, path: String) -> Result<Name, Self::Error> {
            let p = if path == "." || path.is_empty() {
                "/".to_string()
            } else {
                path
            };
            Ok(Name {
                id,
                files: vec![File::dummy(p)],
            })
        }

        async fn opendir(&mut self, id: u32, path: String) -> Result<Handle, Self::Error> {
            self.listed.remove(&path);
            Ok(Handle { id, handle: path })
        }

        async fn readdir(&mut self, id: u32, handle: String) -> Result<Name, Self::Error> {
            if !self.listed.insert(handle.clone()) {
                return Err(StatusCode::Eof);
            }
            let mut files = Vec::new();
            for d in &self.dirs {
                files.push(File::new(d.clone(), attrs(0, true)));
            }
            for (n, data) in &self.files {
                files.push(File::new(n.clone(), attrs(data.len() as u64, false)));
            }
            Ok(Name { id, files })
        }

        async fn open(
            &mut self,
            id: u32,
            filename: String,
            flags: OpenFlags,
            _a: FileAttributes,
        ) -> Result<Handle, Self::Error> {
            let key = base(&filename);
            if flags.contains(OpenFlags::CREATE) {
                let entry = self.files.entry(key.clone()).or_default();
                if flags.contains(OpenFlags::TRUNCATE) {
                    entry.clear();
                }
            } else if !self.files.contains_key(&key) {
                return Err(StatusCode::NoSuchFile);
            }
            Ok(Handle { id, handle: key })
        }

        async fn read(
            &mut self,
            id: u32,
            handle: String,
            offset: u64,
            len: u32,
        ) -> Result<Data, Self::Error> {
            let data = self.files.get(&handle).ok_or(StatusCode::NoSuchFile)?;
            let start = offset as usize;
            if start >= data.len() {
                return Err(StatusCode::Eof);
            }
            let end = (start + len as usize).min(data.len());
            Ok(Data {
                id,
                data: data[start..end].to_vec(),
            })
        }

        async fn write(
            &mut self,
            id: u32,
            handle: String,
            offset: u64,
            data: Vec<u8>,
        ) -> Result<Status, Self::Error> {
            let file = self.files.get_mut(&handle).ok_or(StatusCode::NoSuchFile)?;
            let end = offset as usize + data.len();
            if file.len() < end {
                file.resize(end, 0);
            }
            file[offset as usize..end].copy_from_slice(&data);
            Ok(ok(id))
        }

        async fn close(&mut self, id: u32, _h: String) -> Result<Status, Self::Error> {
            Ok(ok(id))
        }

        async fn stat(&mut self, id: u32, path: String) -> Result<Attrs, Self::Error> {
            let key = base(&path);
            if let Some(d) = self.files.get(&key) {
                return Ok(Attrs {
                    id,
                    attrs: attrs(d.len() as u64, false),
                });
            }
            if key.is_empty() || self.dirs.contains(&key) {
                return Ok(Attrs {
                    id,
                    attrs: attrs(0, true),
                });
            }
            Err(StatusCode::NoSuchFile)
        }

        async fn lstat(&mut self, id: u32, path: String) -> Result<Attrs, Self::Error> {
            self.stat(id, path).await
        }

        async fn fstat(&mut self, id: u32, handle: String) -> Result<Attrs, Self::Error> {
            self.stat(id, handle).await
        }

        async fn mkdir(
            &mut self,
            id: u32,
            path: String,
            _a: FileAttributes,
        ) -> Result<Status, Self::Error> {
            self.dirs.insert(base(&path));
            Ok(ok(id))
        }

        async fn rmdir(&mut self, id: u32, path: String) -> Result<Status, Self::Error> {
            self.dirs.remove(&base(&path));
            Ok(ok(id))
        }

        async fn remove(&mut self, id: u32, path: String) -> Result<Status, Self::Error> {
            self.files
                .remove(&base(&path))
                .ok_or(StatusCode::NoSuchFile)?;
            Ok(ok(id))
        }

        async fn rename(
            &mut self,
            id: u32,
            from: String,
            to: String,
        ) -> Result<Status, Self::Error> {
            let data = self
                .files
                .remove(&base(&from))
                .ok_or(StatusCode::NoSuchFile)?;
            self.files.insert(base(&to), data);
            Ok(ok(id))
        }
    }

    #[derive(Default)]
    struct SftpServer {
        channels: HashMap<ChannelId, Channel<Msg>>,
    }

    impl server::Server for SftpServer {
        type Handler = Self;
        fn new_client(&mut self, _: Option<std::net::SocketAddr>) -> Self {
            Self::default()
        }
    }

    impl server::Handler for SftpServer {
        type Error = russh::Error;

        async fn auth_password(&mut self, _u: &str, _p: &str) -> Result<SAuth, Self::Error> {
            Ok(SAuth::Accept)
        }

        async fn channel_open_session(
            &mut self,
            channel: Channel<Msg>,
            reply: server::ChannelOpenHandle,
            _s: &mut Session,
        ) -> Result<(), Self::Error> {
            self.channels.insert(channel.id(), channel);
            reply.accept().await;
            Ok(())
        }

        async fn subsystem_request(
            &mut self,
            id: ChannelId,
            name: &str,
            session: &mut Session,
        ) -> Result<(), Self::Error> {
            if name == "sftp"
                && let Some(channel) = self.channels.remove(&id)
            {
                session.channel_success(id)?;
                russh_sftp::server::run(channel.into_stream(), Fs::default()).await;
            } else {
                session.channel_failure(id)?;
            }
            Ok(())
        }
    }

    fn start() -> u16 {
        let key = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).unwrap();
        let config = Arc::new(server::Config {
            keys: vec![key],
            auth_rejection_time: Duration::from_millis(1),
            ..Default::default()
        });
        let listener = runtime()
            .block_on(TcpListener::bind(("127.0.0.1", 0)))
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        runtime().spawn(async move {
            let _ = SftpServer::default().run_on_socket(config, &listener).await;
        });
        port
    }

    #[test]
    fn upload_list_download_rename_remove() {
        let port = start();
        let dir = tmp("sftp");
        let sftp = Sftp::connect(cfg(port, &dir, "x", true)).unwrap();
        assert_eq!(sftp.realpath(".").unwrap(), "/");

        let local = dir.join("up.txt");
        std::fs::write(&local, b"hello sftp").unwrap();
        assert_eq!(sftp.upload(&local, "/note.txt").unwrap(), 10);
        sftp.mkdir("/docs").unwrap();

        let list = sftp.list("/").unwrap();
        assert_eq!(list.len(), 2, "{list:?}");
        assert!(
            list[0].is_dir && list[0].name == "docs",
            "dirs first: {list:?}"
        );
        assert_eq!((list[1].name.as_str(), list[1].size), ("note.txt", 10));

        let back = dir.join("down.txt");
        assert_eq!(sftp.download("/note.txt", &back).unwrap(), 10);
        assert_eq!(std::fs::read(&back).unwrap(), b"hello sftp");

        sftp.rename("/note.txt", "/renamed.txt").unwrap();
        assert!(
            sftp.list("/")
                .unwrap()
                .iter()
                .any(|e| e.name == "renamed.txt")
        );
        sftp.remove_file("/renamed.txt").unwrap();
        sftp.remove_dir("/docs").unwrap();
        assert!(sftp.list("/").unwrap().is_empty());
        assert!(sftp.download("/missing", &back).is_err());
    }
}

// ----------------------------------------------------------------------- tunnels

mod tunnel_tests {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    use super::*;
    use crate::tunnel::{open_local, open_socks};

    #[derive(Clone)]
    struct FwdServer;

    impl server::Server for FwdServer {
        type Handler = Self;
        fn new_client(&mut self, _: Option<std::net::SocketAddr>) -> Self {
            Self
        }
    }

    impl server::Handler for FwdServer {
        type Error = russh::Error;

        async fn auth_password(&mut self, _u: &str, _p: &str) -> Result<SAuth, Self::Error> {
            Ok(SAuth::Accept)
        }

        async fn channel_open_direct_tcpip(
            &mut self,
            channel: Channel<Msg>,
            host: &str,
            port: u32,
            _oa: &str,
            _op: u32,
            reply: server::ChannelOpenHandle,
            _s: &mut Session,
        ) -> Result<(), Self::Error> {
            match TcpStream::connect((host, port as u16)).await {
                Ok(mut target) => {
                    reply.accept().await;
                    tokio::spawn(async move {
                        let mut stream = channel.into_stream();
                        let _ = tokio::io::copy_bidirectional(&mut stream, &mut target).await;
                    });
                }
                Err(_) => {
                    let _ = reply.reject(russh::ChannelOpenFailure::ConnectFailed).await;
                }
            }
            Ok(())
        }
    }

    /// Echo server that upper-cases what it receives; returns its port.
    fn start_echo() -> u16 {
        let listener = runtime()
            .block_on(TcpListener::bind(("127.0.0.1", 0)))
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        runtime().spawn(async move {
            while let Ok((mut s, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let mut buf = [0u8; 256];
                    while let Ok(n) = s.read(&mut buf).await {
                        if n == 0 {
                            break;
                        }
                        let up = buf[..n].to_ascii_uppercase();
                        if s.write_all(&up).await.is_err() {
                            break;
                        }
                    }
                });
            }
        });
        port
    }

    fn start_ssh() -> u16 {
        let key = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).unwrap();
        let config = Arc::new(server::Config {
            keys: vec![key],
            auth_rejection_time: Duration::from_millis(1),
            ..Default::default()
        });
        let listener = runtime()
            .block_on(TcpListener::bind(("127.0.0.1", 0)))
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        runtime().spawn(async move {
            let _ = FwdServer.run_on_socket(config, &listener).await;
        });
        port
    }

    async fn roundtrip(port: u16, send: &[u8]) -> Vec<u8> {
        let mut c = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        c.write_all(send).await.unwrap();
        let mut got = vec![0u8; send.len()];
        tokio::time::timeout(Duration::from_secs(5), c.read_exact(&mut got))
            .await
            .unwrap()
            .unwrap();
        got
    }

    #[test]
    fn local_forward_reaches_the_target_through_ssh() {
        let echo = start_echo();
        let ssh = start_ssh();
        let dir = tmp("fwd");
        let tunnel = open_local(cfg(ssh, &dir, "x", true), 0, "127.0.0.1".into(), echo).unwrap();
        let got = runtime().block_on(roundtrip(tunnel.local_port, b"through the tunnel"));
        assert_eq!(got, b"THROUGH THE TUNNEL");
    }

    #[test]
    fn socks5_proxy_connects_by_address_and_by_name() {
        let echo = start_echo();
        let ssh = start_ssh();
        let dir = tmp("socks");
        let tunnel = open_socks(cfg(ssh, &dir, "x", true), 0).unwrap();
        runtime().block_on(async {
            for domain in [false, true] {
                let mut c = TcpStream::connect(("127.0.0.1", tunnel.local_port))
                    .await
                    .unwrap();
                c.write_all(&[5, 1, 0]).await.unwrap();
                let mut r = [0u8; 2];
                c.read_exact(&mut r).await.unwrap();
                assert_eq!(r, [5, 0]);
                let mut req = vec![5, 1, 0];
                if domain {
                    req.extend([3, 9]);
                    req.extend(b"localhost");
                } else {
                    req.extend([1, 127, 0, 0, 1]);
                }
                req.extend(echo.to_be_bytes());
                c.write_all(&req).await.unwrap();
                let mut reply = [0u8; 10];
                c.read_exact(&mut reply).await.unwrap();
                assert_eq!(reply[1], 0, "socks reply {reply:?}");
                c.write_all(b"socks ok").await.unwrap();
                let mut got = [0u8; 8];
                c.read_exact(&mut got).await.unwrap();
                assert_eq!(&got, b"SOCKS OK");
            }
        });
    }

    #[test]
    fn stopping_a_tunnel_closes_the_listener() {
        let echo = start_echo();
        let ssh = start_ssh();
        let dir = tmp("stop");
        let mut tunnel =
            open_local(cfg(ssh, &dir, "x", true), 0, "127.0.0.1".into(), echo).unwrap();
        let port = tunnel.local_port;
        tunnel.stop();
        std::thread::sleep(Duration::from_millis(300));
        assert!(
            runtime()
                .block_on(TcpStream::connect(("127.0.0.1", port)))
                .is_err()
        );
    }
}
