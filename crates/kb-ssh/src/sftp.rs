//! SFTP client (task 4A.5) over the same authenticated connection logic as the shell.
//!
//! Whole-file transfers keep the file in memory; fine for documents and configs, chunked
//! streaming for large files is a follow-up (see PROGRESS.md).

use std::path::Path;

use russh::client;
use russh_sftp::client::SftpSession;
use tokio::io::AsyncWriteExt;

use crate::{Client, Config, SshError, connect_authed, runtime};

/// Biggest file transferred in one piece.
pub const MAX_TRANSFER: u64 = 512 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    /// Unix seconds, when the server reports one.
    pub modified: Option<u32>,
}

pub struct Sftp {
    session: SftpSession,
    // Keeps the SSH connection alive for as long as the SFTP session is used.
    _handle: client::Handle<Client>,
}

fn err(e: impl std::fmt::Display) -> SshError {
    SshError::Other(e.to_string())
}

impl Sftp {
    pub fn connect(cfg: Config) -> Result<Self, SshError> {
        runtime().block_on(async move {
            let handle = connect_authed(&cfg).await?;
            let channel = handle.channel_open_session().await?;
            channel.request_subsystem(true, "sftp").await?;
            let session = SftpSession::new(channel.into_stream()).await.map_err(err)?;
            Ok(Self {
                session,
                _handle: handle,
            })
        })
    }

    /// The server-side absolute path of `path` (`.` = the login directory).
    pub fn realpath(&self, path: &str) -> Result<String, SshError> {
        runtime()
            .block_on(self.session.canonicalize(path))
            .map_err(err)
    }

    /// Directories first, then by name.
    pub fn list(&self, path: &str) -> Result<Vec<Entry>, SshError> {
        runtime().block_on(async {
            let mut out: Vec<Entry> = self
                .session
                .read_dir(path)
                .await
                .map_err(err)?
                .map(|e| {
                    let meta = e.metadata();
                    Entry {
                        name: e.file_name(),
                        is_dir: e.file_type().is_dir(),
                        size: meta.size.unwrap_or(0),
                        modified: meta.mtime,
                    }
                })
                .collect();
            out.sort_by(|a, b| {
                b.is_dir
                    .cmp(&a.is_dir)
                    .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            });
            Ok(out)
        })
    }

    pub fn download(&self, remote: &str, local: &Path) -> Result<u64, SshError> {
        runtime().block_on(async {
            let meta = self.session.metadata(remote).await.map_err(err)?;
            if meta.size.unwrap_or(0) > MAX_TRANSFER {
                return Err(SshError::Other(
                    "file too large for a single transfer".into(),
                ));
            }
            let data = self.session.read(remote).await.map_err(err)?;
            std::fs::write(local, &data).map_err(err)?;
            Ok(data.len() as u64)
        })
    }

    pub fn upload(&self, local: &Path, remote: &str) -> Result<u64, SshError> {
        let meta = std::fs::metadata(local).map_err(err)?;
        if meta.len() > MAX_TRANSFER {
            return Err(SshError::Other(
                "file too large for a single transfer".into(),
            ));
        }
        let data = std::fs::read(local).map_err(err)?;
        runtime().block_on(async {
            // `create` = CREATE | TRUNCATE | WRITE; plain `write` would fail for a new file.
            let mut file = self.session.create(remote).await.map_err(err)?;
            file.write_all(&data).await.map_err(err)?;
            file.close().await.map_err(err)
        })?;
        Ok(data.len() as u64)
    }

    pub fn mkdir(&self, path: &str) -> Result<(), SshError> {
        runtime()
            .block_on(self.session.create_dir(path))
            .map_err(err)
    }

    pub fn remove_file(&self, path: &str) -> Result<(), SshError> {
        runtime()
            .block_on(self.session.remove_file(path))
            .map_err(err)
    }

    pub fn remove_dir(&self, path: &str) -> Result<(), SshError> {
        runtime()
            .block_on(self.session.remove_dir(path))
            .map_err(err)
    }

    pub fn rename(&self, from: &str, to: &str) -> Result<(), SshError> {
        runtime()
            .block_on(self.session.rename(from, to))
            .map_err(err)
    }
}
