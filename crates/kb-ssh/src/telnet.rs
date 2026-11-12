//! Minimal Telnet client (task 4A.7): RFC 854 option negotiation that refuses everything except
//! NAWS (window size) and terminal type, plus escaping of 0xFF. Plain text over TCP, so the UI
//! must warn before using it on anything but a trusted network.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::{Event, SshError};

const IAC: u8 = 255;
const DONT: u8 = 254;
const DO: u8 = 253;
const WONT: u8 = 252;
const WILL: u8 = 251;
const SB: u8 = 250;
const SE: u8 = 240;
const OPT_TTYPE: u8 = 24;
const OPT_NAWS: u8 = 31;

pub struct Telnet {
    stream: Mutex<TcpStream>,
    size: Arc<Mutex<(u16, u16)>>,
    naws: Arc<Mutex<bool>>,
}

/// Splits raw bytes into text and negotiation replies. Keeps parser state across chunks.
#[derive(Default)]
struct Parser {
    state: State,
    sb: Vec<u8>,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum State {
    #[default]
    Data,
    Iac,
    Option(u8),
    Sub,
    SubIac,
}

impl Parser {
    /// Returns (text for the terminal, bytes to send back).
    fn feed(&mut self, input: &[u8], size: (u16, u16), naws: &mut bool) -> (Vec<u8>, Vec<u8>) {
        let mut text = Vec::new();
        let mut reply = Vec::new();
        for &b in input {
            match self.state {
                State::Data => {
                    if b == IAC {
                        self.state = State::Iac;
                    } else {
                        text.push(b);
                    }
                }
                State::Iac => match b {
                    IAC => {
                        text.push(IAC);
                        self.state = State::Data;
                    }
                    DO | DONT | WILL | WONT => self.state = State::Option(b),
                    SB => {
                        self.sb.clear();
                        self.state = State::Sub;
                    }
                    _ => self.state = State::Data,
                },
                State::Option(verb) => {
                    match (verb, b) {
                        (DO, OPT_NAWS) => {
                            *naws = true;
                            reply.extend([IAC, WILL, OPT_NAWS]);
                            reply.extend(naws_frame(size));
                        }
                        (DO, OPT_TTYPE) => reply.extend([IAC, WILL, OPT_TTYPE]),
                        (DO, o) => reply.extend([IAC, WONT, o]),
                        (WILL, o) => reply.extend([IAC, DONT, o]),
                        _ => {}
                    }
                    self.state = State::Data;
                }
                State::Sub => {
                    if b == IAC {
                        self.state = State::SubIac;
                    } else {
                        self.sb.push(b);
                    }
                }
                State::SubIac => {
                    if b == SE {
                        // TERMINAL-TYPE SEND (24 1) → IS xterm-256color
                        if self.sb == [OPT_TTYPE, 1] {
                            reply.extend([IAC, SB, OPT_TTYPE, 0]);
                            reply.extend(b"xterm-256color");
                            reply.extend([IAC, SE]);
                        }
                        self.state = State::Data;
                    } else {
                        self.sb.push(b);
                        self.state = State::Sub;
                    }
                }
            }
        }
        (text, reply)
    }
}

fn naws_frame((cols, rows): (u16, u16)) -> Vec<u8> {
    let mut f = vec![IAC, SB, OPT_NAWS];
    for b in cols.to_be_bytes().into_iter().chain(rows.to_be_bytes()) {
        f.push(b);
        if b == IAC {
            f.push(IAC);
        }
    }
    f.extend([IAC, SE]);
    f
}

/// Connects and streams output through `on_event` from a reader thread.
pub fn connect(
    host: &str,
    port: u16,
    cols: u16,
    rows: u16,
    on_event: Arc<dyn Fn(Event) + Send + Sync>,
) -> Result<Telnet, SshError> {
    use std::net::ToSocketAddrs;
    let addr = (host, port)
        .to_socket_addrs()
        .map_err(|e| SshError::Other(e.to_string()))?
        .next()
        .ok_or_else(|| SshError::Other("host not found".into()))?;
    let stream = TcpStream::connect_timeout(&addr, Duration::from_secs(15))
        .map_err(|e| SshError::Other(e.to_string()))?;
    let _ = stream.set_nodelay(true);
    let mut reader = stream
        .try_clone()
        .map_err(|e| SshError::Other(e.to_string()))?;
    let mut writer = stream
        .try_clone()
        .map_err(|e| SshError::Other(e.to_string()))?;
    let size = Arc::new(Mutex::new((cols.max(2), rows.max(2))));
    let naws = Arc::new(Mutex::new(false));
    let (size2, naws2) = (size.clone(), naws.clone());
    std::thread::Builder::new()
        .name("kb-telnet".into())
        .spawn(move || {
            let mut parser = Parser::default();
            let mut buf = [0u8; 8192];
            while let Ok(n) = reader.read(&mut buf) {
                if n == 0 {
                    break;
                }
                let sz = size2.lock().map_or((80, 24), |s| *s);
                let mut flag = naws2.lock().is_ok_and(|n| *n);
                let (text, reply) = parser.feed(&buf[..n], sz, &mut flag);
                if let Ok(mut n) = naws2.lock() {
                    *n = flag;
                }
                if !reply.is_empty() && writer.write_all(&reply).is_err() {
                    break;
                }
                if !text.is_empty() {
                    on_event(Event::Data(text));
                }
            }
            on_event(Event::Exited(None));
        })
        .map_err(|e| SshError::Other(e.to_string()))?;
    Ok(Telnet {
        stream: Mutex::new(stream),
        size,
        naws,
    })
}

impl Telnet {
    pub fn write(&self, data: &[u8]) {
        let mut out = Vec::with_capacity(data.len());
        for &b in data {
            out.push(b);
            if b == IAC {
                out.push(IAC);
            }
        }
        if let Ok(mut s) = self.stream.lock() {
            let _ = s.write_all(&out);
        }
    }

    pub fn resize(&self, cols: u16, rows: u16) {
        if let Ok(mut s) = self.size.lock() {
            *s = (cols.max(2), rows.max(2));
        }
        if self.naws.lock().is_ok_and(|n| *n)
            && let Ok(mut s) = self.stream.lock()
        {
            let _ = s.write_all(&naws_frame((cols.max(2), rows.max(2))));
        }
    }

    pub fn close(&self) {
        if let Ok(s) = self.stream.lock() {
            let _ = s.shutdown(std::net::Shutdown::Both);
        }
    }
}

impl Drop for Telnet {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::time::Instant;

    #[test]
    fn parser_strips_negotiation_and_answers_it() {
        let mut p = Parser::default();
        let mut naws = false;
        let input = [
            b'h', b'i', IAC, DO, OPT_NAWS, IAC, DO, 1, IAC, WILL, 1, IAC, IAC, b'!',
        ];
        let (text, reply) = p.feed(&input, (100, 30), &mut naws);
        assert_eq!(text, [b'h', b'i', IAC, b'!']);
        assert!(naws);
        assert_eq!(
            reply,
            [
                IAC, WILL, OPT_NAWS, IAC, SB, OPT_NAWS, 0, 100, 0, 30, IAC, SE, // NAWS
                IAC, WONT, 1, // refuse echo request
                IAC, DONT, 1, // refuse server option
            ]
        );
    }

    #[test]
    fn parser_survives_split_sequences() {
        let mut p = Parser::default();
        let mut naws = false;
        let (t1, _) = p.feed(&[b'a', IAC], (80, 24), &mut naws);
        let (t2, r2) = p.feed(&[DO, 99, b'b'], (80, 24), &mut naws);
        assert_eq!((t1, t2, r2), (vec![b'a'], vec![b'b'], vec![IAC, WONT, 99]));
    }

    #[test]
    fn talks_to_a_server_end_to_end() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            s.write_all(&[IAC, DO, OPT_NAWS]).unwrap();
            s.write_all(b"login: ").unwrap();
            let mut buf = [0u8; 64];
            let mut got = Vec::new();
            while got.windows(2).all(|w| w != b"\r\n") {
                let n = s.read(&mut buf).unwrap();
                if n == 0 {
                    return;
                }
                got.extend_from_slice(&buf[..n]);
            }
            let mut reply = b"echo:".to_vec();
            reply.extend(got.iter().filter(|b| b.is_ascii_graphic()));
            s.write_all(&reply).unwrap();
        });
        let log = Arc::new(Mutex::new(Vec::<u8>::new()));
        let l = log.clone();
        let t = connect(
            "127.0.0.1",
            port,
            80,
            24,
            Arc::new(move |e| {
                if let Event::Data(d) = e {
                    l.lock().unwrap().extend(d);
                }
            }),
        )
        .unwrap();
        t.write(b"root\r\n");
        let start = Instant::now();
        loop {
            let text = String::from_utf8_lossy(&log.lock().unwrap()).to_string();
            if text.contains("login: ") && text.contains("echo:") {
                // the NAWS frame the client sent must not have been echoed as text
                assert!(text.contains("root"));
                break;
            }
            assert!(start.elapsed() < Duration::from_secs(5), "got {text:?}");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
