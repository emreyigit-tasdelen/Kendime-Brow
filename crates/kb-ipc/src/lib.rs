//! IPC framing (architecture §3.3): every message is a 4-byte little-endian length prefix
//! followed by the payload (protobuf, added in task 1.9).

use std::io::{self, Read, Write};

/// Upper bound for a single framed message. Large data (bitmaps, frames) goes through
/// shared memory instead; messages only carry handles + offsets.
pub const MAX_FRAME_LEN: usize = 16 * 1024 * 1024;

/// Writes one length-prefixed frame.
pub fn write_frame<W: Write>(w: &mut W, payload: &[u8]) -> io::Result<()> {
    if payload.len() > MAX_FRAME_LEN {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "frame too large",
        ));
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
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "frame too large",
        ));
    }
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf)?;
    Ok(Some(buf))
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
}
