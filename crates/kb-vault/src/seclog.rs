//! Encrypted append-only session log (task 4A.9, architecture 10.5).
//!
//! Each record is `nonce(24) | length(4, LE) | ciphertext`, sealed with XChaCha20-Poly1305. The
//! record's sequence number is authenticated, so dropping, reordering or duplicating records in
//! the middle of a file is detected; truncating the tail is not (append-only logs cannot).

use chacha20poly1305::aead::{Aead, Payload};
use chacha20poly1305::{KeyInit, XChaCha20Poly1305};

use crate::VaultError;

const AAD_PREFIX: &[u8] = b"kb-vault/log/v1/";

fn aad(seq: u64) -> Vec<u8> {
    let mut a = AAD_PREFIX.to_vec();
    a.extend(seq.to_le_bytes());
    a
}

pub struct LogWriter {
    cipher: XChaCha20Poly1305,
    seq: u64,
}

impl LogWriter {
    /// `first_seq` is the number of records already in the file (0 for a new log).
    pub fn new(key: &[u8; 32], first_seq: u64) -> Self {
        Self {
            cipher: XChaCha20Poly1305::new(key.into()),
            seq: first_seq,
        }
    }

    /// Seals one entry and returns the bytes to append to the log file.
    pub fn seal(&mut self, entry: &[u8]) -> Result<Vec<u8>, VaultError> {
        let mut nonce = [0u8; 24];
        getrandom::fill(&mut nonce).map_err(|_| VaultError::Crypto)?;
        let ct = self
            .cipher
            .encrypt(
                (&nonce).into(),
                Payload {
                    msg: entry,
                    aad: &aad(self.seq),
                },
            )
            .map_err(|_| VaultError::Crypto)?;
        self.seq += 1;
        let mut out = Vec::with_capacity(28 + ct.len());
        out.extend(nonce);
        out.extend((ct.len() as u32).to_le_bytes());
        out.extend(ct);
        Ok(out)
    }
}

/// Decrypts every record of a log file.
pub fn open_all(key: &[u8; 32], mut file: &[u8]) -> Result<Vec<Vec<u8>>, VaultError> {
    let cipher = XChaCha20Poly1305::new(key.into());
    let mut out = Vec::new();
    while !file.is_empty() {
        if file.len() < 28 {
            return Err(VaultError::Format("truncated log record".into()));
        }
        let nonce: [u8; 24] = file[..24].try_into().expect("checked length");
        let len = u32::from_le_bytes(file[24..28].try_into().expect("checked length")) as usize;
        let body = file
            .get(28..28 + len)
            .ok_or_else(|| VaultError::Format("truncated log record".into()))?;
        let plain = cipher
            .decrypt(
                (&nonce).into(),
                Payload {
                    msg: body,
                    aad: &aad(out.len() as u64),
                },
            )
            .map_err(|_| VaultError::Crypto)?;
        out.push(plain);
        file = &file[28 + len..];
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: [u8; 32] = [7; 32];

    fn log(entries: &[&str]) -> Vec<Vec<u8>> {
        let mut w = LogWriter::new(&KEY, 0);
        entries
            .iter()
            .map(|e| w.seal(e.as_bytes()).unwrap())
            .collect()
    }

    #[test]
    fn round_trips_in_order() {
        let recs = log(&["ls -la\r\n", "whoami\r\n", ""]);
        let file: Vec<u8> = recs.concat();
        let back = open_all(&KEY, &file).unwrap();
        assert_eq!(
            back,
            [b"ls -la\r\n".to_vec(), b"whoami\r\n".to_vec(), vec![]]
        );
        assert!(!file.windows(6).any(|w| w == b"whoami"));
    }

    #[test]
    fn wrong_key_and_tampering_are_rejected() {
        let file = log(&["secret"]).concat();
        assert!(open_all(&[8; 32], &file).is_err());
        let mut bad = file.clone();
        let last = bad.len() - 1;
        bad[last] ^= 1;
        assert!(open_all(&KEY, &bad).is_err());
    }

    #[test]
    fn reordering_and_dropping_middle_records_is_detected() {
        let r = log(&["a", "b", "c"]);
        assert!(open_all(&KEY, &[r[1].clone(), r[0].clone(), r[2].clone()].concat()).is_err());
        assert!(open_all(&KEY, &[r[0].clone(), r[2].clone()].concat()).is_err());
    }

    #[test]
    fn truncated_records_are_rejected_and_appending_continues_the_sequence() {
        let first = log(&["a"]).concat();
        assert!(open_all(&KEY, &first[..first.len() - 1]).is_err());
        let mut w = LogWriter::new(&KEY, 1);
        let mut file = first;
        file.extend(w.seal(b"b").unwrap());
        assert_eq!(
            open_all(&KEY, &file).unwrap(),
            [b"a".to_vec(), b"b".to_vec()]
        );
    }
}
