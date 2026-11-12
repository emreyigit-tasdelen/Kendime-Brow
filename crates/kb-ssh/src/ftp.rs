//! FTP and explicit FTPS client (task 4A.7b) on top of suppaftp. Plain FTP is unencrypted, so
//! the UI warns; FTPS verifies the server certificate against the bundled web roots.

use std::io::Cursor;
use std::net::ToSocketAddrs;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use suppaftp::RustlsFtpStream;
use suppaftp::list::File;

use crate::sftp::Entry;

/// Whole-file transfers are held in memory, as with SFTP.
const MAX_FILE: usize = 512 * 1024 * 1024;

pub struct Ftp {
    inner: Mutex<RustlsFtpStream>,
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn tls_config() -> Arc<rustls::ClientConfig> {
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let cfg = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .expect("ring supports the default protocol versions")
    .with_root_certificates(roots)
    .with_no_client_auth();
    Arc::new(cfg)
}

impl Ftp {
    /// `secure` upgrades the control connection with AUTH TLS before logging in.
    pub fn connect(
        host: &str,
        port: u16,
        user: &str,
        password: &str,
        secure: bool,
    ) -> Result<Self, String> {
        let addr = (host, port)
            .to_socket_addrs()
            .map_err(err)?
            .next()
            .ok_or("host not found")?;
        let mut s = RustlsFtpStream::connect_timeout(addr, Duration::from_secs(15)).map_err(err)?;
        if secure {
            s = s
                .into_secure(suppaftp::RustlsConnector::from(tls_config()), host)
                .map_err(err)?;
        }
        s.login(user, password).map_err(err)?;
        Ok(Self {
            inner: Mutex::new(s),
        })
    }

    fn with<T>(
        &self,
        f: impl FnOnce(&mut RustlsFtpStream) -> suppaftp::FtpResult<T>,
    ) -> Result<T, String> {
        let mut s = self.inner.lock().map_err(|_| "poisoned".to_string())?;
        f(&mut s).map_err(err)
    }

    pub fn pwd(&self) -> Result<String, String> {
        self.with(|s| s.pwd())
    }

    pub fn list(&self, path: &str) -> Result<Vec<Entry>, String> {
        let lines = self.with(|s| s.list(Some(path).filter(|p| !p.is_empty())))?;
        let mut out: Vec<Entry> = lines
            .iter()
            .filter_map(|l| File::try_from(l.as_str()).ok())
            .filter(|f| f.name() != "." && f.name() != "..")
            .map(|f| Entry {
                name: f.name().to_string(),
                is_dir: f.is_directory(),
                size: f.size() as u64,
                modified: f
                    .modified()
                    .duration_since(std::time::UNIX_EPOCH)
                    .ok()
                    .map(|d| d.as_secs() as u32),
            })
            .collect();
        out.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
        Ok(out)
    }

    pub fn download(&self, remote: &str, local: &std::path::Path) -> Result<u64, String> {
        if let Ok(n) = self.with(|s| s.size(remote))
            && n > MAX_FILE
        {
            return Err("file too large (512 MB limit)".into());
        }
        let data = self.with(|s| s.retr_as_buffer(remote))?.into_inner();
        std::fs::write(local, &data).map_err(err)?;
        Ok(data.len() as u64)
    }

    pub fn upload(&self, local: &std::path::Path, remote: &str) -> Result<u64, String> {
        let data = std::fs::read(local).map_err(err)?;
        if data.len() > MAX_FILE {
            return Err("file too large (512 MB limit)".into());
        }
        let mut cur = Cursor::new(data);
        self.with(|s| s.put_file(remote, &mut cur))
    }

    pub fn mkdir(&self, path: &str) -> Result<(), String> {
        self.with(|s| s.mkdir(path))
    }
    pub fn remove_file(&self, path: &str) -> Result<(), String> {
        self.with(|s| s.rm(path))
    }
    pub fn remove_dir(&self, path: &str) -> Result<(), String> {
        self.with(|s| s.rmdir(path))
    }
    pub fn rename(&self, from: &str, to: &str) -> Result<(), String> {
        self.with(|s| s.rename(from, to))
    }
}

impl Drop for Ftp {
    fn drop(&mut self) {
        if let Ok(mut s) = self.inner.lock() {
            let _ = s.quit();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;

    /// Canned control-channel server: greeting, login, PWD, MKD, QUIT.
    #[test]
    fn logs_in_and_runs_simple_commands() {
        let l = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = l.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let (s, _) = l.accept().unwrap();
            let mut w = s.try_clone().unwrap();
            w.write_all(b"220 hi\r\n").unwrap();
            for line in BufReader::new(s).lines() {
                let line = line.unwrap();
                let reply: &[u8] = match line.split(' ').next().unwrap() {
                    "USER" => b"331 pw\r\n",
                    "PASS" => b"230 ok\r\n",
                    "TYPE" => b"200 ok\r\n",
                    "PWD" => b"257 \"/home/x\" is cwd\r\n",
                    "MKD" => b"257 \"made\" created\r\n",
                    "QUIT" => {
                        w.write_all(b"221 bye\r\n").unwrap();
                        return;
                    }
                    _ => b"502 no\r\n",
                };
                w.write_all(reply).unwrap();
            }
        });
        let f = Ftp::connect("127.0.0.1", port, "u", "p", false).unwrap();
        assert_eq!(f.pwd().unwrap(), "/home/x");
        f.mkdir("made").unwrap();
    }

    #[test]
    fn refused_connection_is_an_error() {
        assert!(Ftp::connect("127.0.0.1", 1, "u", "p", false).is_err());
    }
}
