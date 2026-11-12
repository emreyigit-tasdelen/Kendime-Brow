//! Platform IPC endpoints (architecture §3.3 / §3.8): Windows named pipes, Unix domain sockets
//! elsewhere. One listener accepts one connection (one module host per endpoint).
//!
//! Windows pipes get an explicit security descriptor: full access for the current user only,
//! plus a LOW mandatory label so sandboxed (low integrity) module hosts can connect. The pipe
//! also rejects remote clients and must be the first instance of its random name; the token
//! handshake in `kb-ipc` authenticates the peer.

use std::io;

use tokio::io::{AsyncRead, AsyncWrite};

/// A connected, bidirectional byte stream.
pub trait Stream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Stream for T {}

/// Random endpoint name: `kb-<pid>-<module>-<16 hex>`.
pub fn endpoint_name(module: &str) -> io::Result<String> {
    let mut rnd = [0u8; 8];
    getrandom::fill(&mut rnd).map_err(io::Error::other)?;
    let hex: String = rnd.iter().map(|b| format!("{b:02x}")).collect();
    let module: String = module
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect();
    Ok(format!("kb-{}-{module}-{hex}", std::process::id()))
}

#[cfg(windows)]
mod imp {
    use super::*;
    use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeServer, ServerOptions};

    fn path(name: &str) -> String {
        format!(r"\\.\pipe\{name}")
    }

    pub struct Listener {
        server: Option<NamedPipeServer>,
    }

    /// `D:P(A;;GA;;;<user>)S:(ML;;NW;;;LW)` — protected DACL for the current user, low label.
    pub fn pipe_sddl() -> io::Result<String> {
        let sid = crate::sandbox::current_user_sid()?;
        Ok(format!("D:P(A;;GA;;;{sid})S:(ML;;NW;;;LW)"))
    }

    pub fn listen(name: &str) -> io::Result<Listener> {
        use windows_sys::Win32::Foundation::LocalFree;
        use windows_sys::Win32::Security::Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW;
        use windows_sys::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};

        let sddl: Vec<u16> = pipe_sddl()?
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let mut sd: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        // SDDL_REVISION_1 = 1
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                1,
                &mut sd,
                std::ptr::null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        let mut sa = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: sd,
            bInheritHandle: 0,
        };
        let server = unsafe {
            ServerOptions::new()
                .first_pipe_instance(true)
                .reject_remote_clients(true)
                .create_with_security_attributes_raw(
                    path(name),
                    (&mut sa as *mut SECURITY_ATTRIBUTES).cast(),
                )
        };
        unsafe { LocalFree(sd) };
        Ok(Listener {
            server: Some(server?),
        })
    }

    impl Listener {
        pub async fn accept(mut self) -> io::Result<Box<dyn Stream>> {
            let server = self
                .server
                .take()
                .ok_or_else(|| io::Error::other("already accepted"))?;
            server.connect().await?;
            Ok(Box::new(server))
        }
    }

    pub async fn connect(name: &str) -> io::Result<Box<dyn Stream>> {
        Ok(Box::new(ClientOptions::new().open(path(name))?))
    }
}

#[cfg(unix)]
mod imp {
    use super::*;
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    use std::path::PathBuf;
    use tokio::net::{UnixListener, UnixStream};

    fn dir() -> PathBuf {
        let base = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        base.join("kendime-brow")
    }

    fn path(name: &str) -> PathBuf {
        dir().join(format!("{name}.sock"))
    }

    pub struct Listener {
        listener: UnixListener,
        path: PathBuf,
    }

    pub fn listen(name: &str) -> io::Result<Listener> {
        let d = dir();
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&d)?;
        std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o700))?;
        let path = path(name);
        let listener = UnixListener::bind(&path)?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        Ok(Listener { listener, path })
    }

    impl Listener {
        pub async fn accept(self) -> io::Result<Box<dyn Stream>> {
            let (stream, _) = self.listener.accept().await?;
            Ok(Box::new(stream))
        }
    }

    impl Drop for Listener {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }

    pub async fn connect(name: &str) -> io::Result<Box<dyn Stream>> {
        Ok(Box::new(UnixStream::connect(path(name)).await?))
    }
}

#[cfg(windows)]
pub use imp::pipe_sddl;
pub use imp::{Listener, connect, listen};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_names_are_random_and_sanitized() {
        let a = endpoint_name("kb-doc").unwrap();
        let b = endpoint_name("kb-doc").unwrap();
        assert_ne!(a, b);
        assert!(a.starts_with(&format!("kb-{}-kb-doc-", std::process::id())));
        assert!(endpoint_name("../x y").unwrap().contains("-xy-"));
    }
}
