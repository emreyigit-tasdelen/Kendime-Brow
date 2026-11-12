//! Throwaway SSH server for live checks: user `tester`, password `pw`, echoes input.
//! Run: `cargo run -p kb-ssh --example testserver -- 2222`
use std::sync::Arc;
use std::time::Duration;

use russh::keys::{Algorithm, HashAlg, PrivateKey};
use russh::server::{self, Auth, Msg, Server as _, Session};
use russh::{Channel, ChannelId};

#[derive(Clone)]
struct Srv;

impl server::Server for Srv {
    type Handler = Self;
    fn new_client(&mut self, _: Option<std::net::SocketAddr>) -> Self {
        Self
    }
}

impl server::Handler for Srv {
    type Error = russh::Error;

    async fn auth_password(&mut self, user: &str, pw: &str) -> Result<Auth, Self::Error> {
        Ok(if user == "tester" && pw == "pw" {
            Auth::Accept
        } else {
            Auth::reject()
        })
    }

    async fn channel_open_session(
        &mut self,
        _c: Channel<Msg>,
        reply: server::ChannelOpenHandle,
        _s: &mut Session,
    ) -> Result<(), Self::Error> {
        reply.accept().await;
        Ok(())
    }

    async fn pty_request(
        &mut self,
        ch: ChannelId,
        _t: &str,
        _c: u32,
        _r: u32,
        _pw: u32,
        _ph: u32,
        _m: &[(russh::Pty, u32)],
        s: &mut Session,
    ) -> Result<(), Self::Error> {
        s.channel_success(ch)?;
        Ok(())
    }

    async fn shell_request(&mut self, ch: ChannelId, s: &mut Session) -> Result<(), Self::Error> {
        s.channel_success(ch)?;
        s.data(ch, b"welcome to the kb-ssh test server\r\n".to_vec())?;
        Ok(())
    }

    async fn data(&mut self, ch: ChannelId, d: &[u8], s: &mut Session) -> Result<(), Self::Error> {
        s.data(ch, d.to_vec())?;
        Ok(())
    }
}

#[tokio::main]
async fn main() {
    let port: u16 = std::env::args()
        .nth(1)
        .and_then(|p| p.parse().ok())
        .unwrap_or(2222);
    let key = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).unwrap();
    println!(
        "listening on 127.0.0.1:{port}, host key {}",
        key.public_key().fingerprint(HashAlg::Sha256)
    );
    let config = Arc::new(server::Config {
        keys: vec![key],
        auth_rejection_time: Duration::from_millis(1),
        ..Default::default()
    });
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .unwrap();
    let _ = Srv.run_on_socket(config, &listener).await;
}
