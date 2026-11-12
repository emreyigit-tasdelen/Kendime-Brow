//! Test module host used by `tests/launch.rs`: connects back to the shell with the endpoint and
//! token from the environment, then serves a few test methods.
//!
//! Methods: `echo` (body back), `try-write` (body = path; tries to create that file),
//! `exit` (exits with code body[0]), `crash` (aborts).

use kb_ipc::{ENV_ENDPOINT, ENV_TOKEN, Token, client_handshake, connect, pb, recv, send};

#[tokio::main(flavor = "current_thread")]
async fn main() -> std::io::Result<()> {
    let endpoint = std::env::var(ENV_ENDPOINT).map_err(std::io::Error::other)?;
    let token = std::env::var(ENV_TOKEN)
        .ok()
        .and_then(|t| Token::from_hex(&t))
        .ok_or_else(|| std::io::Error::other("missing token"))?;
    let mut s = connect(&endpoint).await?;
    client_handshake(&mut *s, &token, "kb-echo-host").await?;
    loop {
        let req: pb::Envelope = recv(&mut *s).await?;
        let (body, error) = match req.method.as_str() {
            "echo" => (req.body.clone(), String::new()),
            "try-write" => {
                let path = String::from_utf8_lossy(&req.body).to_string();
                match std::fs::write(&path, b"x") {
                    Ok(()) => (b"written".to_vec(), String::new()),
                    Err(e) => (Vec::new(), e.to_string()),
                }
            }
            "exit" => std::process::exit(req.body.first().copied().unwrap_or(0) as i32),
            "crash" => std::process::abort(),
            other => (Vec::new(), format!("unknown method {other}")),
        };
        send(
            &mut *s,
            &pb::Envelope {
                id: req.id,
                method: req.method,
                body,
                error,
            },
        )
        .await?;
    }
}
