//! Launching a sandboxed module host and supervising it (architecture §3.1–3.2, task 1.10).

use std::io;
use std::sync::Arc;
use std::time::Duration;

use kb_platform::sandbox::{self, Child, SpawnSpec};

use crate::{ENV_ENDPOINT, ENV_TOKEN, Stream, Token, accept_handshake, endpoint_name, listen};

/// How long a freshly started host gets to connect and complete the handshake.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// A running, authenticated module host.
pub struct Host {
    pub child: Arc<Child>,
    pub stream: Box<dyn Stream>,
    pub module: String,
}

/// Starts `spec` in the sandbox with a fresh endpoint + token and waits for its handshake.
pub async fn launch(module: &str, mut spec: SpawnSpec) -> io::Result<Host> {
    let name = endpoint_name(module)?;
    let token = Token::generate()?;
    let listener = listen(&name)?;
    spec.env.push((ENV_ENDPOINT.into(), name));
    spec.env.push((ENV_TOKEN.into(), token.to_hex()));
    let child = Arc::new(sandbox::spawn(&spec)?);

    let handshake = async {
        let mut stream = listener.accept().await?;
        let module = accept_handshake(&mut *stream, &token).await?;
        Ok::<_, io::Error>((stream, module))
    };
    match tokio::time::timeout(CONNECT_TIMEOUT, handshake).await {
        Ok(Ok((stream, module))) => Ok(Host {
            child,
            stream,
            module,
        }),
        Ok(Err(e)) => {
            let _ = child.kill();
            Err(e)
        }
        Err(_) => {
            let _ = child.kill();
            Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "module host did not connect",
            ))
        }
    }
}

/// Restart policy: a crashed host is restarted after `backoff`, at most `max_restarts` times.
#[derive(Debug, Clone, Copy)]
pub struct RestartPolicy {
    pub max_restarts: u32,
    pub backoff: Duration,
}

impl Default for RestartPolicy {
    fn default() -> Self {
        Self {
            max_restarts: 3,
            backoff: Duration::from_millis(500),
        }
    }
}

/// Waits for the host process to exit (on a blocking thread). Returns the exit code.
pub async fn wait_exit(child: Arc<Child>) -> io::Result<u32> {
    tokio::task::spawn_blocking(move || child.wait())
        .await
        .map_err(io::Error::other)?
}

/// Exit code 0 is a clean shutdown; anything else is a crash that warrants a restart.
pub fn should_restart(exit_code: u32, restarts_so_far: u32, policy: &RestartPolicy) -> bool {
    exit_code != 0 && restarts_so_far < policy.max_restarts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restart_policy() {
        let p = RestartPolicy::default();
        assert!(!should_restart(0, 0, &p));
        assert!(should_restart(1, 0, &p));
        assert!(should_restart(0xC000_0409, 2, &p));
        assert!(!should_restart(1, 3, &p));
    }
}
