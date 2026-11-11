//! IPC between kb-shell and module hosts (architecture §3.3).
//!
//! - Framing: 4-byte little-endian length prefix + payload (sync and async variants).
//! - Messages: protobuf, generated from `proto/kb/ipc/v1/ipc.proto` (prost + protox).
//! - Handshake: the connecting side sends `Hello` with the single-use 256-bit token the shell
//!   handed it at launch; the listener answers `HelloAck` and drops the connection on mismatch.

use std::io::{self, Read, Write};

use prost::Message;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub use kb_platform::ipc::{Listener, Stream, connect, endpoint_name, listen};

/// Generated protobuf types (`kb.ipc.v1`).
pub mod pb {
    include!(concat!(env!("OUT_DIR"), "/kb.ipc.v1.rs"));
}

pub const PROTOCOL_VERSION: u32 = 1;

/// Upper bound for a single framed message. Large data (bitmaps, frames) goes through
/// shared memory instead; messages only carry handles + offsets.
pub const MAX_FRAME_LEN: usize = 16 * 1024 * 1024;

fn too_large(kind: io::ErrorKind) -> io::Error {
    io::Error::new(kind, "frame too large")
}

// ------------------------------------------------------------------ sync framing

/// Writes one length-prefixed frame.
pub fn write_frame<W: Write>(w: &mut W, payload: &[u8]) -> io::Result<()> {
    if payload.len() > MAX_FRAME_LEN {
        return Err(too_large(io::ErrorKind::InvalidInput));
    }
    w.write_all(&(payload.len() as u32).to_le_bytes())?;
    w.write_all(payload)
}

/// Reads one length-prefixed frame. Returns `Ok(None)` on a clean EOF before a frame starts.
pub fn read_frame<R: Read>(r: &mut R) -> io::Result<Option<Vec<u8>>> {
    let mut len = [0u8; 4];
    let mut filled = 0;
    while filled < len.len() {
        match r.read(&mut len[filled..])? {
            0 if filled == 0 => return Ok(None),
            0 => return Err(io::ErrorKind::UnexpectedEof.into()),
            n => filled += n,
        }
    }
    let len = u32::from_le_bytes(len) as usize;
    if len > MAX_FRAME_LEN {
        return Err(too_large(io::ErrorKind::InvalidData));
    }
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf)?;
    Ok(Some(buf))
}

// ------------------------------------------------------------------ async framing

pub async fn write_frame_async<W: AsyncWrite + Unpin + ?Sized>(
    w: &mut W,
    payload: &[u8],
) -> io::Result<()> {
    if payload.len() > MAX_FRAME_LEN {
        return Err(too_large(io::ErrorKind::InvalidInput));
    }
    w.write_all(&(payload.len() as u32).to_le_bytes()).await?;
    w.write_all(payload).await?;
    w.flush().await
}

pub async fn read_frame_async<R: AsyncRead + Unpin + ?Sized>(
    r: &mut R,
) -> io::Result<Option<Vec<u8>>> {
    let mut len = [0u8; 4];
    let mut filled = 0;
    while filled < len.len() {
        match r.read(&mut len[filled..]).await? {
            0 if filled == 0 => return Ok(None),
            0 => return Err(io::ErrorKind::UnexpectedEof.into()),
            n => filled += n,
        }
    }
    let len = u32::from_le_bytes(len) as usize;
    if len > MAX_FRAME_LEN {
        return Err(too_large(io::ErrorKind::InvalidData));
    }
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf).await?;
    Ok(Some(buf))
}

pub async fn send<W: AsyncWrite + Unpin + ?Sized, M: Message>(
    w: &mut W,
    msg: &M,
) -> io::Result<()> {
    write_frame_async(w, &msg.encode_to_vec()).await
}

pub async fn recv<R: AsyncRead + Unpin + ?Sized, M: Message + Default>(r: &mut R) -> io::Result<M> {
    let frame = read_frame_async(r)
        .await?
        .ok_or_else(|| io::Error::from(io::ErrorKind::UnexpectedEof))?;
    M::decode(frame.as_slice()).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

// ------------------------------------------------------------------ handshake

/// Single-use 256-bit secret shared with a child process at launch.
#[derive(Clone, PartialEq, Eq)]
pub struct Token([u8; 32]);

impl Token {
    pub fn generate() -> io::Result<Self> {
        let mut t = [0u8; 32];
        getrandom::fill(&mut t).map_err(io::Error::other)?;
        Ok(Self(t))
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Hex form for passing through an environment variable / inherited handle.
    pub fn to_hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }

    pub fn from_hex(s: &str) -> Option<Self> {
        if s.len() != 64 || !s.is_ascii() {
            return None;
        }
        let mut t = [0u8; 32];
        for (i, b) in t.iter_mut().enumerate() {
            *b = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok()?;
        }
        Some(Self(t))
    }

    /// Constant-time comparison.
    pub fn matches(&self, other: &[u8]) -> bool {
        other.len() == 32
            && self
                .0
                .iter()
                .zip(other)
                .fold(0u8, |acc, (a, b)| acc | (a ^ b))
                == 0
    }
}

impl std::fmt::Debug for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Token(<redacted>)")
    }
}

/// Listener side: expects `Hello` with the right token, answers `HelloAck`.
/// Returns the peer's module name.
pub async fn accept_handshake<S: AsyncRead + AsyncWrite + Unpin + ?Sized>(
    stream: &mut S,
    expected: &Token,
) -> io::Result<String> {
    let hello: pb::Hello = recv(stream).await?;
    let error = if !expected.matches(&hello.token) {
        Some("bad token")
    } else if hello.protocol_version != PROTOCOL_VERSION {
        Some("unsupported protocol version")
    } else {
        None
    };
    send(
        stream,
        &pb::HelloAck {
            ok: error.is_none(),
            error: error.unwrap_or_default().to_string(),
        },
    )
    .await?;
    match error {
        None => Ok(hello.module),
        Some(e) => Err(io::Error::new(io::ErrorKind::PermissionDenied, e)),
    }
}

/// Connecting side: sends `Hello`, waits for an `ok` `HelloAck`.
pub async fn client_handshake<S: AsyncRead + AsyncWrite + Unpin + ?Sized>(
    stream: &mut S,
    token: &Token,
    module: &str,
) -> io::Result<()> {
    send(
        stream,
        &pb::Hello {
            token: token.as_bytes().to_vec(),
            module: module.to_string(),
            protocol_version: PROTOCOL_VERSION,
        },
    )
    .await?;
    let ack: pb::HelloAck = recv(stream).await?;
    if ack.ok {
        Ok(())
    } else {
        Err(io::Error::new(io::ErrorKind::PermissionDenied, ack.error))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn round_trip_multiple_frames() {
        let mut buf = Vec::new();
        write_frame(&mut buf, b"hello").unwrap();
        write_frame(&mut buf, b"").unwrap();
        write_frame(&mut buf, &[7u8; 1000]).unwrap();
        let mut r = Cursor::new(buf);
        assert_eq!(read_frame(&mut r).unwrap().unwrap(), b"hello");
        assert_eq!(read_frame(&mut r).unwrap().unwrap(), b"");
        assert_eq!(read_frame(&mut r).unwrap().unwrap(), vec![7u8; 1000]);
        assert!(read_frame(&mut r).unwrap().is_none());
    }

    #[test]
    fn truncated_frame_is_an_error() {
        let mut buf = Vec::new();
        write_frame(&mut buf, b"hello").unwrap();
        buf.truncate(6);
        assert!(read_frame(&mut Cursor::new(buf)).is_err());
        assert!(read_frame(&mut Cursor::new(vec![1u8, 0])).is_err());
    }

    #[test]
    fn oversized_frames_are_rejected() {
        let big = (MAX_FRAME_LEN as u32 + 1).to_le_bytes();
        assert!(read_frame(&mut Cursor::new(big.to_vec())).is_err());
    }

    #[test]
    fn token_hex_and_compare() {
        let t = Token::generate().unwrap();
        let back = Token::from_hex(&t.to_hex()).unwrap();
        assert!(t == back);
        assert!(t.matches(back.as_bytes()));
        assert!(!t.matches(&[0u8; 32]));
        assert!(!t.matches(&t.as_bytes()[..31]));
        assert!(Token::from_hex("zz").is_none());
        assert_eq!(format!("{t:?}"), "Token(<redacted>)");
    }
}
