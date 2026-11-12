//! Encrypted credential vault (architecture §10.2, task 4A.1).
//!
//! ```text
//! master key (32 random bytes)
//!   └─ wrapped by a key derived from the master password with Argon2id → stored in the header
//! records: each record is XChaCha20-Poly1305 encrypted with its own random nonce; the record
//!   id and kind are authenticated as associated data (a record cannot be swapped or retyped)
//! ```
//!
//! The file is a single JSON document written atomically. This crate has no network and no UI;
//! it runs inside the `kb-vault` host process (separate-process wiring is task 4A.1b). Unlocking
//! with the OS keyring / Windows Hello is task 4A.1c.

pub mod csv;
pub mod kdbx;
pub mod origin;
pub mod seclog;

use std::collections::BTreeMap;
use std::path::Path;

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, Payload};
use chacha20poly1305::{KeyInit, XChaCha20Poly1305};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};

pub const FORMAT_VERSION: u32 = 1;

/// Argon2id cost (architecture: m = 256 MiB, t = 3). Tests use cheaper parameters.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct KdfParams {
    pub memory_kib: u32,
    pub iterations: u32,
    pub parallelism: u32,
}

impl KdfParams {
    pub const STRONG: Self = Self {
        memory_kib: 256 * 1024,
        iterations: 3,
        parallelism: 1,
    };
    pub const FAST_FOR_TESTS: Self = Self {
        memory_kib: 64,
        iterations: 1,
        parallelism: 1,
    };
}

#[derive(Debug, PartialEq, Eq)]
pub enum VaultError {
    WrongPasswordOrCorrupt,
    Locked,
    NotFound,
    Format(String),
    Io(String),
    Crypto,
}

impl std::fmt::Display for VaultError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WrongPasswordOrCorrupt => write!(f, "wrong password or damaged vault"),
            Self::Locked => write!(f, "vault is locked"),
            Self::NotFound => write!(f, "entry not found"),
            Self::Format(m) => write!(f, "unsupported vault file: {m}"),
            Self::Io(m) => write!(f, "vault file error: {m}"),
            Self::Crypto => write!(f, "cryptographic failure"),
        }
    }
}

impl std::error::Error for VaultError {}

/// What a record holds. Part of the authenticated data.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    Login,
    SshKey,
    Note,
    Token,
}

impl Kind {
    fn tag(self) -> &'static str {
        match self {
            Self::Login => "login",
            Self::SshKey => "ssh-key",
            Self::Note => "note",
            Self::Token => "token",
        }
    }
}

/// Secret fields of a record (zeroed on drop).
#[derive(Debug, Clone, Default, Serialize, Deserialize, Zeroize, PartialEq, Eq)]
#[zeroize(drop)]
pub struct Secret {
    pub title: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub notes: String,
}

#[derive(Serialize, Deserialize, Clone)]
struct StoredRecord {
    kind: Kind,
    nonce: String,
    ciphertext: String,
}

#[derive(Serialize, Deserialize)]
struct File {
    version: u32,
    kdf: KdfParams,
    salt: String,
    wrap_nonce: String,
    wrapped_key: String,
    records: BTreeMap<String, StoredRecord>,
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex(s: &str) -> Result<Vec<u8>, VaultError> {
    if !s.len().is_multiple_of(2) || !s.is_ascii() {
        return Err(VaultError::Format("bad hex".into()));
    }
    (0..s.len() / 2)
        .map(|i| {
            u8::from_str_radix(&s[i * 2..i * 2 + 2], 16)
                .map_err(|_| VaultError::Format("bad hex".into()))
        })
        .collect()
}

fn nonce24(s: &str) -> Result<[u8; 24], VaultError> {
    unhex(s)?
        .try_into()
        .map_err(|_| VaultError::Format("nonce length".into()))
}

fn random<const N: usize>() -> Result<[u8; N], VaultError> {
    let mut b = [0u8; N];
    getrandom::fill(&mut b).map_err(|_| VaultError::Crypto)?;
    Ok(b)
}

fn derive(password: &str, salt: &[u8], p: &KdfParams) -> Result<Zeroizing<[u8; 32]>, VaultError> {
    let params = Params::new(p.memory_kib, p.iterations, p.parallelism, Some(32))
        .map_err(|_| VaultError::Crypto)?;
    let mut out = Zeroizing::new([0u8; 32]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(password.as_bytes(), salt, &mut *out)
        .map_err(|_| VaultError::Crypto)?;
    Ok(out)
}

fn aead(key: &[u8; 32]) -> XChaCha20Poly1305 {
    XChaCha20Poly1305::new(key.into())
}

fn aad(id: &str, kind: Kind) -> Vec<u8> {
    format!("kb-vault/v1/{}/{id}", kind.tag()).into_bytes()
}

pub struct Vault {
    file: File,
    key: Option<Zeroizing<[u8; 32]>>,
}

impl Vault {
    /// Creates an unlocked, empty vault protected by `password`.
    pub fn create(password: &str, kdf: KdfParams) -> Result<Self, VaultError> {
        let master = Zeroizing::new(random::<32>()?);
        let salt = random::<16>()?;
        let wrap_nonce = random::<24>()?;
        let kek = derive(password, &salt, &kdf)?;
        let wrapped = aead(&kek)
            .encrypt(
                (&wrap_nonce).into(),
                Payload {
                    msg: &master[..],
                    aad: b"kb-vault/v1/master",
                },
            )
            .map_err(|_| VaultError::Crypto)?;
        Ok(Self {
            file: File {
                version: FORMAT_VERSION,
                kdf,
                salt: hex(&salt),
                wrap_nonce: hex(&wrap_nonce),
                wrapped_key: hex(&wrapped),
                records: BTreeMap::new(),
            },
            key: Some(master),
        })
    }

    pub fn from_json(json: &str) -> Result<Self, VaultError> {
        let file: File =
            serde_json::from_str(json).map_err(|e| VaultError::Format(e.to_string()))?;
        if file.version != FORMAT_VERSION {
            return Err(VaultError::Format(format!("version {}", file.version)));
        }
        Ok(Self { file, key: None })
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(&self.file).unwrap_or_default()
    }

    pub fn open(path: &Path) -> Result<Self, VaultError> {
        Self::from_json(&std::fs::read_to_string(path).map_err(|e| VaultError::Io(e.to_string()))?)
    }

    /// Atomic save (temp file + rename); on Unix the file is private to the user.
    pub fn save(&self, path: &Path) -> Result<(), VaultError> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| VaultError::Io(e.to_string()))?;
        }
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, self.to_json()).map_err(|e| VaultError::Io(e.to_string()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
        }
        std::fs::rename(&tmp, path).map_err(|e| VaultError::Io(e.to_string()))
    }

    pub fn is_unlocked(&self) -> bool {
        self.key.is_some()
    }

    pub fn unlock(&mut self, password: &str) -> Result<(), VaultError> {
        let salt = unhex(&self.file.salt)?;
        let nonce = nonce24(&self.file.wrap_nonce)?;
        let wrapped = unhex(&self.file.wrapped_key)?;
        let kek = derive(password, &salt, &self.file.kdf)?;
        let master = aead(&kek)
            .decrypt(
                (&nonce).into(),
                Payload {
                    msg: &wrapped,
                    aad: b"kb-vault/v1/master",
                },
            )
            .map_err(|_| VaultError::WrongPasswordOrCorrupt)?;
        let mut key = Zeroizing::new([0u8; 32]);
        if master.len() != 32 {
            return Err(VaultError::WrongPasswordOrCorrupt);
        }
        key.copy_from_slice(&master);
        self.key = Some(key);
        Ok(())
    }

    /// Forgets the master key (zeroed on drop).
    pub fn lock(&mut self) {
        self.key = None;
    }

    fn key(&self) -> Result<&[u8; 32], VaultError> {
        self.key.as_deref().ok_or(VaultError::Locked)
    }

    /// Adds or replaces the record `id`.
    pub fn put(&mut self, id: &str, kind: Kind, secret: &Secret) -> Result<(), VaultError> {
        let key = *self.key()?;
        let nonce = random::<24>()?;
        let plain = Zeroizing::new(serde_json::to_vec(secret).map_err(|_| VaultError::Crypto)?);
        let ct = aead(&key)
            .encrypt(
                (&nonce).into(),
                Payload {
                    msg: &plain,
                    aad: &aad(id, kind),
                },
            )
            .map_err(|_| VaultError::Crypto)?;
        self.file.records.insert(
            id.to_string(),
            StoredRecord {
                kind,
                nonce: hex(&nonce),
                ciphertext: hex(&ct),
            },
        );
        Ok(())
    }

    pub fn get(&self, id: &str) -> Result<(Kind, Secret), VaultError> {
        let key = self.key()?;
        let r = self.file.records.get(id).ok_or(VaultError::NotFound)?;
        let nonce = nonce24(&r.nonce)?;
        let plain = Zeroizing::new(
            aead(key)
                .decrypt(
                    (&nonce).into(),
                    Payload {
                        msg: &unhex(&r.ciphertext)?,
                        aad: &aad(id, r.kind),
                    },
                )
                .map_err(|_| VaultError::WrongPasswordOrCorrupt)?,
        );
        let secret: Secret =
            serde_json::from_slice(&plain).map_err(|e| VaultError::Format(e.to_string()))?;
        Ok((r.kind, secret))
    }

    /// Titles of all records (decrypts them; requires the vault to be unlocked).
    pub fn list(&self) -> Result<Vec<(String, Kind, String)>, VaultError> {
        self.file
            .records
            .keys()
            .map(|id| self.get(id).map(|(k, s)| (id.clone(), k, s.title.clone())))
            .collect()
    }

    pub fn remove(&mut self, id: &str) -> Result<(), VaultError> {
        self.file
            .records
            .remove(id)
            .map(|_| ())
            .ok_or(VaultError::NotFound)
    }

    pub fn len(&self) -> usize {
        self.file.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.file.records.is_empty()
    }

    /// Re-wraps the master key under a new password (records are untouched).
    pub fn change_password(&mut self, new_password: &str) -> Result<(), VaultError> {
        let master = *self.key()?;
        let salt = random::<16>()?;
        let nonce = random::<24>()?;
        let kek = derive(new_password, &salt, &self.file.kdf)?;
        let wrapped = aead(&kek)
            .encrypt(
                (&nonce).into(),
                Payload {
                    msg: &master,
                    aad: b"kb-vault/v1/master",
                },
            )
            .map_err(|_| VaultError::Crypto)?;
        self.file.salt = hex(&salt);
        self.file.wrap_nonce = hex(&nonce);
        self.file.wrapped_key = hex(&wrapped);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secret(title: &str, pw: &str) -> Secret {
        let mut s = Secret::default();
        s.title = title.into();
        s.username = "emre".into();
        s.password = pw.into();
        s
    }

    fn vault() -> Vault {
        Vault::create("correct horse", KdfParams::FAST_FOR_TESTS).unwrap()
    }

    #[test]
    fn round_trip_through_json_and_lock() {
        let mut v = vault();
        v.put("srv1", Kind::Login, &secret("Server 1", "hunter2"))
            .unwrap();
        let json = v.to_json();
        assert!(
            !json.contains("hunter2") && !json.contains("Server 1"),
            "plaintext leaked"
        );

        let mut again = Vault::from_json(&json).unwrap();
        assert_eq!(again.get("srv1"), Err(VaultError::Locked));
        assert_eq!(
            again.unlock("wrong"),
            Err(VaultError::WrongPasswordOrCorrupt)
        );
        again.unlock("correct horse").unwrap();
        let (kind, s) = again.get("srv1").unwrap();
        assert_eq!((kind, s.password.as_str()), (Kind::Login, "hunter2"));
        assert_eq!(
            again.list().unwrap(),
            vec![("srv1".to_string(), Kind::Login, "Server 1".to_string())]
        );
        again.lock();
        assert!(!again.is_unlocked());
        assert_eq!(
            again.put("x", Kind::Note, &secret("n", "")),
            Err(VaultError::Locked)
        );
    }

    #[test]
    fn records_cannot_be_swapped_or_retyped() {
        let mut v = vault();
        v.put("a", Kind::Login, &secret("A", "pa")).unwrap();
        v.put("b", Kind::Login, &secret("B", "pb")).unwrap();
        // Swap the ciphertexts of two records.
        let (a, b) = (v.file.records["a"].clone(), v.file.records["b"].clone());
        v.file.records.insert("a".into(), b);
        v.file.records.insert("b".into(), a);
        assert_eq!(v.get("a"), Err(VaultError::WrongPasswordOrCorrupt));
        // Change the kind of a record.
        let mut v = vault();
        v.put("a", Kind::Login, &secret("A", "pa")).unwrap();
        v.file.records.get_mut("a").unwrap().kind = Kind::Token;
        assert_eq!(v.get("a"), Err(VaultError::WrongPasswordOrCorrupt));
    }

    #[test]
    fn tampering_is_detected() {
        let mut v = vault();
        v.put("a", Kind::Note, &secret("A", "p")).unwrap();
        let rec = v.file.records.get_mut("a").unwrap();
        let mut ct = unhex(&rec.ciphertext).unwrap();
        ct[0] ^= 1;
        rec.ciphertext = hex(&ct);
        assert_eq!(v.get("a"), Err(VaultError::WrongPasswordOrCorrupt));
    }

    #[test]
    fn nonces_are_unique_and_put_replaces() {
        let mut v = vault();
        v.put("a", Kind::Login, &secret("A", "1")).unwrap();
        let n1 = v.file.records["a"].nonce.clone();
        v.put("a", Kind::Login, &secret("A", "2")).unwrap();
        assert_ne!(n1, v.file.records["a"].nonce);
        assert_eq!(v.len(), 1);
        assert_eq!(v.get("a").unwrap().1.password, "2");
        v.remove("a").unwrap();
        assert!(v.is_empty());
        assert_eq!(v.remove("a"), Err(VaultError::NotFound));
    }

    #[test]
    fn password_change_keeps_records() {
        let mut v = vault();
        v.put("a", Kind::Login, &secret("A", "pw")).unwrap();
        v.change_password("new pass").unwrap();
        let mut again = Vault::from_json(&v.to_json()).unwrap();
        assert!(again.unlock("correct horse").is_err());
        again.unlock("new pass").unwrap();
        assert_eq!(again.get("a").unwrap().1.password, "pw");
    }

    #[test]
    fn file_save_and_open() {
        let dir = std::env::temp_dir().join(format!("kb-vault-test-{}", std::process::id()));
        let path = dir.join("vault.json");
        let mut v = vault();
        v.put("a", Kind::SshKey, &secret("key", "PRIVATE")).unwrap();
        v.save(&path).unwrap();
        let mut again = Vault::open(&path).unwrap();
        again.unlock("correct horse").unwrap();
        assert_eq!(again.get("a").unwrap().0, Kind::SshKey);
        assert!(Vault::from_json("{\"version\":99}").is_err());
        assert!(Vault::from_json("not json").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
