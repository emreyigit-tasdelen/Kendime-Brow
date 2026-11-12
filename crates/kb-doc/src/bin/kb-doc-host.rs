//! `kb-doc-host`: PDF module host process (architecture §3.1). Launched sandboxed by the shell
//! with the IPC endpoint and token in the environment.

use kb_ipc::{ENV_ENDPOINT, ENV_TOKEN, Token, client_handshake, connect};

#[tokio::main(flavor = "current_thread")]
async fn main() -> std::io::Result<()> {
    let endpoint = std::env::var(ENV_ENDPOINT).map_err(std::io::Error::other)?;
    let token = std::env::var(ENV_TOKEN)
        .ok()
        .and_then(|t| Token::from_hex(&t))
        .ok_or_else(|| std::io::Error::other("missing token"))?;
    let pdfium = kb_doc::load_pdfium().map_err(std::io::Error::other)?;
    let mut stream = connect(&endpoint).await?;
    client_handshake(&mut *stream, &token, "kb-doc").await?;
    kb_doc::service::serve(&mut *stream, &pdfium).await
}
