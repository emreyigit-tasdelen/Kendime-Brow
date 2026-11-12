//! SSH port forwarding (task 4A.6): local forwards (`-L`) and a dynamic SOCKS5 proxy (`-D`).
//!
//! Listeners bind to 127.0.0.1 only. A tunnel lives until it is dropped or `stop`ped.

use std::sync::Arc;

use russh::client;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;

use crate::{Client, Config, SshError, connect_authed, runtime};

pub struct Tunnel {
    /// Port the tunnel listens on (useful when 0 was requested).
    pub local_port: u16,
    stop: Option<oneshot::Sender<()>>,
}

impl Tunnel {
    pub fn stop(&mut self) {
        if let Some(tx) = self.stop.take() {
            let _ = tx.send(());
        }
    }
}

impl Drop for Tunnel {
    fn drop(&mut self) {
        self.stop();
    }
}

#[derive(Clone)]
enum Mode {
    Local { host: String, port: u16 },
    Socks,
}

/// `127.0.0.1:bind_port` → `remote_host:remote_port` as seen from the SSH server.
pub fn open_local(
    cfg: Config,
    bind_port: u16,
    remote_host: String,
    remote_port: u16,
) -> Result<Tunnel, SshError> {
    start(
        cfg,
        bind_port,
        Mode::Local {
            host: remote_host,
            port: remote_port,
        },
    )
}

/// SOCKS5 proxy on `127.0.0.1:bind_port`; every connection leaves from the SSH server.
pub fn open_socks(cfg: Config, bind_port: u16) -> Result<Tunnel, SshError> {
    start(cfg, bind_port, Mode::Socks)
}

fn start(cfg: Config, bind_port: u16, mode: Mode) -> Result<Tunnel, SshError> {
    runtime().block_on(async move {
        let handle = Arc::new(connect_authed(&cfg).await?);
        let listener = TcpListener::bind(("127.0.0.1", bind_port))
            .await
            .map_err(|e| SshError::Other(format!("cannot listen on port {bind_port}: {e}")))?;
        let local_port = listener
            .local_addr()
            .map_err(|e| SshError::Other(e.to_string()))?
            .port();
        let (stop, mut stopped) = oneshot::channel();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut stopped => break,
                    accepted = listener.accept() => {
                        let Ok((sock, peer)) = accepted else { break };
                        let handle = handle.clone();
                        let mode = mode.clone();
                        tokio::spawn(async move {
                            let _ = serve(sock, peer, handle, mode).await;
                        });
                    }
                }
            }
            let _ = handle
                .disconnect(russh::Disconnect::ByApplication, "", "en")
                .await;
        });
        Ok(Tunnel {
            local_port,
            stop: Some(stop),
        })
    })
}

async fn serve(
    mut sock: TcpStream,
    peer: std::net::SocketAddr,
    handle: Arc<client::Handle<Client>>,
    mode: Mode,
) -> Result<(), SshError> {
    let socks = matches!(mode, Mode::Socks);
    let (host, port) = match mode {
        Mode::Local { host, port } => (host, port),
        Mode::Socks => socks5_handshake(&mut sock).await?,
    };
    let channel = handle
        .channel_open_direct_tcpip(
            host,
            u32::from(port),
            peer.ip().to_string(),
            u32::from(peer.port()),
        )
        .await;
    let channel = match channel {
        Ok(c) => c,
        Err(e) => {
            if socks {
                // 5 = connection refused
                let _ = sock.write_all(&[5, 5, 0, 1, 0, 0, 0, 0, 0, 0]).await;
            }
            return Err(e.into());
        }
    };
    if socks {
        sock.write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0])
            .await
            .map_err(|e| SshError::Other(e.to_string()))?;
    }
    let mut stream = channel.into_stream();
    let _ = tokio::io::copy_bidirectional(&mut sock, &mut stream).await;
    Ok(())
}

/// Minimal SOCKS5 (RFC 1928): no authentication, CONNECT only. The final reply is sent by the
/// caller once the SSH channel is open.
async fn socks5_handshake(sock: &mut TcpStream) -> Result<(String, u16), SshError> {
    let io = |e: std::io::Error| SshError::Other(e.to_string());
    let mut head = [0u8; 2];
    sock.read_exact(&mut head).await.map_err(io)?;
    if head[0] != 5 {
        return Err(SshError::Other("not a SOCKS5 client".into()));
    }
    let mut methods = vec![0u8; head[1] as usize];
    sock.read_exact(&mut methods).await.map_err(io)?;
    if !methods.contains(&0) {
        sock.write_all(&[5, 0xff]).await.map_err(io)?;
        return Err(SshError::Other("no acceptable SOCKS auth method".into()));
    }
    sock.write_all(&[5, 0]).await.map_err(io)?;

    let mut req = [0u8; 4];
    sock.read_exact(&mut req).await.map_err(io)?;
    if req[1] != 1 {
        sock.write_all(&[5, 7, 0, 1, 0, 0, 0, 0, 0, 0])
            .await
            .map_err(io)?;
        return Err(SshError::Other("only SOCKS5 CONNECT is supported".into()));
    }
    let host = match req[3] {
        1 => {
            let mut a = [0u8; 4];
            sock.read_exact(&mut a).await.map_err(io)?;
            std::net::Ipv4Addr::from(a).to_string()
        }
        3 => {
            let mut len = [0u8; 1];
            sock.read_exact(&mut len).await.map_err(io)?;
            let mut name = vec![0u8; len[0] as usize];
            sock.read_exact(&mut name).await.map_err(io)?;
            String::from_utf8(name).map_err(|_| SshError::Other("bad host name".into()))?
        }
        4 => {
            let mut a = [0u8; 16];
            sock.read_exact(&mut a).await.map_err(io)?;
            std::net::Ipv6Addr::from(a).to_string()
        }
        _ => return Err(SshError::Other("unsupported SOCKS address type".into())),
    };
    let mut p = [0u8; 2];
    sock.read_exact(&mut p).await.map_err(io)?;
    Ok((host, u16::from_be_bytes(p)))
}
