//! Local terminal sessions (architecture §10, task 4A.2): a shell on a pseudo terminal
//! (ConPTY on Windows, `openpty` elsewhere) streamed to `kb://term` pages.
//!
//! Security: a local shell runs with the user's full rights, so only `kb://` pages (the bridge
//! is closed to web content) can open one, and a session dies with the query that opened it.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};

type Emit = Arc<dyn Fn(Output) + Send + Sync>;

/// What a session produces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Output {
    Data(Vec<u8>),
    Exited(Option<u32>),
}

struct Session {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    killer: Box<dyn portable_pty::ChildKiller + Send + Sync>,
}

static SESSIONS: OnceLock<Mutex<HashMap<u32, Session>>> = OnceLock::new();
static NEXT: AtomicU32 = AtomicU32::new(1);

fn sessions() -> &'static Mutex<HashMap<u32, Session>> {
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The user's shell: `%COMSPEC%`-style PowerShell on Windows, `$SHELL` elsewhere.
pub fn default_shell() -> (String, Vec<String>) {
    if cfg!(windows) {
        for candidate in ["pwsh.exe", "powershell.exe"] {
            if which(candidate) {
                return (candidate.into(), vec!["-NoLogo".into()]);
            }
        }
        (
            std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".into()),
            Vec::new(),
        )
    } else {
        (
            std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into()),
            Vec::new(),
        )
    }
}

fn which(exe: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(exe).is_file()))
}

/// Starts a shell session and returns its id. `emit` receives output on a reader thread.
pub fn open(cols: u16, rows: u16, emit: Emit) -> Result<u32, String> {
    let pty = native_pty_system()
        .openpty(PtySize {
            rows: rows.max(2),
            cols: cols.max(2),
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| e.to_string())?;
    let (program, args) = default_shell();
    let mut cmd = CommandBuilder::new(program);
    cmd.args(args);
    if let Some(home) = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }) {
        cmd.cwd(home);
    }
    cmd.env("TERM", "xterm-256color");
    let mut child = pty.slave.spawn_command(cmd).map_err(|e| e.to_string())?;
    let killer = child.clone_killer();
    let mut reader = pty.master.try_clone_reader().map_err(|e| e.to_string())?;
    let writer = pty.master.take_writer().map_err(|e| e.to_string())?;
    drop(pty.slave);

    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    sessions().lock().map_err(|_| "poisoned")?.insert(
        id,
        Session {
            master: pty.master,
            writer,
            killer,
        },
    );
    // Reader: forwards output until the pseudo terminal closes.
    std::thread::Builder::new()
        .name(format!("kb-term-{id}-out"))
        .spawn({
            let emit = emit.clone();
            move || {
                let mut buf = [0u8; 8192];
                while let Ok(n) = reader.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    emit(Output::Data(buf[..n].to_vec()));
                }
            }
        })
        .map_err(|e| e.to_string())?;
    // Waiter: when the shell exits, drop the session (closing the master unblocks the reader on
    // ConPTY, which does not report EOF by itself) and announce the exit.
    std::thread::Builder::new()
        .name(format!("kb-term-{id}-wait"))
        .spawn(move || {
            let code = child.wait().ok().map(|s| s.exit_code());
            if let Ok(mut s) = sessions().lock() {
                s.remove(&id);
            }
            emit(Output::Exited(code));
        })
        .map_err(|e| e.to_string())?;
    Ok(id)
}

pub fn write(id: u32, data: &[u8]) -> Result<(), String> {
    let mut s = sessions().lock().map_err(|_| "poisoned")?;
    let session = s.get_mut(&id).ok_or("no such session")?;
    session.writer.write_all(data).map_err(|e| e.to_string())?;
    session.writer.flush().map_err(|e| e.to_string())
}

pub fn resize(id: u32, cols: u16, rows: u16) -> Result<(), String> {
    let s = sessions().lock().map_err(|_| "poisoned")?;
    let session = s.get(&id).ok_or("no such session")?;
    session
        .master
        .resize(PtySize {
            rows: rows.max(2),
            cols: cols.max(2),
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| e.to_string())
}

/// Kills the shell; the reader thread then reports `Exited`.
pub fn close(id: u32) {
    if let Ok(mut s) = sessions().lock()
        && let Some(mut session) = s.remove(&id)
    {
        let _ = session.killer.kill();
    }
}

pub fn count() -> usize {
    sessions().lock().map_or(0, |s| s.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;
    use std::time::Duration;

    #[test]
    fn runs_a_command_through_the_pty() {
        let (tx, rx) = channel();
        let tx = Mutex::new(tx);
        let id = open(
            100,
            30,
            Arc::new(move |o| {
                let _ = tx.lock().unwrap().send(o);
            }),
        )
        .unwrap();
        write(id, b"echo kb-terminal-ok\r\n").unwrap();
        let mut seen = String::new();
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        while std::time::Instant::now() < deadline && !seen.contains("kb-terminal-ok") {
            if let Ok(Output::Data(d)) = rx.recv_timeout(Duration::from_millis(500)) {
                let text = String::from_utf8_lossy(&d).to_string();
                // ConPTY asks for the cursor position at start-up and waits for the answer, as
                // a real terminal emulator (xterm.js) would give it.
                if text.contains("[6n") {
                    write(id, b"[1;1R").unwrap();
                }
                seen.push_str(&text);
            }
        }
        assert!(seen.contains("kb-terminal-ok"), "no output, got: {seen:?}");
        resize(id, 80, 24).unwrap();
        write(id, b"exit\r\n").unwrap();
        let mut exited = false;
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        while std::time::Instant::now() < deadline && !exited {
            exited = matches!(
                rx.recv_timeout(Duration::from_millis(500)),
                Ok(Output::Exited(_))
            );
        }
        assert!(exited, "shell did not exit");
        assert_eq!(count(), 0);
        assert!(write(id, b"x").is_err());
    }
}
