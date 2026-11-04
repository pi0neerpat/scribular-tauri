//! Database key handling (parity spec 01 R2/R5/R6/R12): where the key lives,
//! and the open decision table as a pure function so every row is tested.

use std::{fmt::Write, io::Read, path::Path, sync::Mutex};

use serde::Serialize;
use ts_rs::TS;

pub const KEY_LEN: usize = 32;
pub type Key = [u8; KEY_LEN];

/// Why the store can't be opened (R7). The webview picks its copy from this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum LockReason {
    KeychainDenied,
    KeyMissing,
    WrongKey,
    MigrationFailed,
    NotSqlcipherBuild,
    KeychainWriteFailed,
}

/// Where the database key lives. Errors carry a message for the log only.
pub trait KeyStore: Send + Sync {
    fn get(&self) -> Result<Option<Key>, String>;
    fn set(&self, key: &Key) -> Result<(), String>;
}

/// For tests and `bin/bench.rs`: the real Keychain is never touched there.
#[derive(Default)]
pub struct MemoryKeyStore {
    key: Mutex<Option<Key>>,
    /// Makes `set` fail, for the R6/AS5 paths.
    pub fail_set: bool,
}

impl MemoryKeyStore {
    pub fn with_key(key: Key) -> Self {
        Self {
            key: Mutex::new(Some(key)),
            fail_set: false,
        }
    }
}

impl KeyStore for MemoryKeyStore {
    fn get(&self) -> Result<Option<Key>, String> {
        Ok(*self.key.lock().unwrap())
    }

    fn set(&self, key: &Key) -> Result<(), String> {
        if self.fail_set {
            return Err("set refused".into());
        }
        *self.key.lock().unwrap() = Some(*key);
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbState {
    Absent,
    /// Starts with the SQLite magic header: a prototype plaintext DB.
    Plaintext,
    /// Anything else: SQLCipher, or not a database at all. Either way only the
    /// right key opens it, and it is never overwritten.
    Encrypted,
}

impl DbState {
    /// A zero-length file holds nothing, so it counts as absent.
    pub fn of(path: &Path) -> std::io::Result<Self> {
        let mut file = match std::fs::File::open(path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::Absent),
            Err(e) => return Err(e),
        };
        let mut head = Vec::with_capacity(16);
        file.by_ref().take(16).read_to_end(&mut head)?;
        Ok(match head.as_slice() {
            [] => Self::Absent,
            b"SQLite format 3\0" => Self::Plaintext,
            _ => Self::Encrypted,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Plan {
    /// Generate a key, store it, read it back, then create the DB.
    CreateWithNewKey,
    /// Orphan key from an earlier crash: reuse it for a new DB.
    CreateWithStoredKey,
    /// Open with the stored key; a failure there is `WrongKey`.
    Open,
    /// Encrypt the prototype DB (R8), with the stored key or a new one.
    Migrate {
        new_key: bool,
    },
    Locked(LockReason),
}

/// The R5 table, minus its runtime row (a stored key that fails to decrypt).
pub fn plan_open(db: DbState, key_present: bool) -> Plan {
    match (db, key_present) {
        (DbState::Absent, false) => Plan::CreateWithNewKey,
        (DbState::Absent, true) => Plan::CreateWithStoredKey,
        (DbState::Encrypted, true) => Plan::Open,
        (DbState::Encrypted, false) => Plan::Locked(LockReason::KeyMissing),
        (DbState::Plaintext, present) => Plan::Migrate { new_key: !present },
    }
}

/// The `PRAGMA key` value for a raw key: `"x'<64 hex>'"` (R2).
pub fn pragma_key(key: &Key) -> String {
    let mut s = String::with_capacity(KEY_LEN * 2 + 5);
    s.push_str("\"x'");
    for b in key {
        write!(s, "{b:02x}").unwrap();
    }
    s.push_str("'\"");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as4_every_row_of_the_decision_table() {
        use DbState::*;
        assert_eq!(plan_open(Absent, false), Plan::CreateWithNewKey);
        assert_eq!(plan_open(Absent, true), Plan::CreateWithStoredKey);
        assert_eq!(plan_open(Encrypted, true), Plan::Open);
        // Never a new key for an existing encrypted file.
        assert_eq!(
            plan_open(Encrypted, false),
            Plan::Locked(LockReason::KeyMissing)
        );
        assert_eq!(plan_open(Plaintext, false), Plan::Migrate { new_key: true });
        assert_eq!(plan_open(Plaintext, true), Plan::Migrate { new_key: false });
    }

    #[test]
    fn classifies_files_by_header() {
        let dir = std::env::temp_dir().join(format!("scribular-key-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("db");
        let _ = std::fs::remove_file(&p);
        assert_eq!(DbState::of(&p).unwrap(), DbState::Absent);
        std::fs::write(&p, b"").unwrap();
        assert_eq!(DbState::of(&p).unwrap(), DbState::Absent);
        std::fs::write(&p, b"SQLite format 3\0rest of page").unwrap();
        assert_eq!(DbState::of(&p).unwrap(), DbState::Plaintext);
        std::fs::write(&p, [0x5au8; 4096]).unwrap();
        assert_eq!(DbState::of(&p).unwrap(), DbState::Encrypted);
        std::fs::write(&p, b"SQLite").unwrap();
        assert_eq!(DbState::of(&p).unwrap(), DbState::Encrypted, "truncated");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn pragma_key_is_raw_hex() {
        let mut key = [0u8; KEY_LEN];
        key[0] = 0xab;
        key[31] = 0x01;
        let p = pragma_key(&key);
        assert_eq!(p.len(), 64 + 5);
        assert!(p.starts_with("\"x'ab00"));
        assert!(p.ends_with("01'\""));
    }

    #[test]
    fn memory_store_round_trips_and_can_refuse() {
        let s = MemoryKeyStore::default();
        assert_eq!(s.get().unwrap(), None);
        s.set(&[7; KEY_LEN]).unwrap();
        assert_eq!(s.get().unwrap(), Some([7; KEY_LEN]));
        let refusing = MemoryKeyStore {
            fail_set: true,
            ..Default::default()
        };
        assert!(refusing.set(&[1; KEY_LEN]).is_err());
        assert_eq!(refusing.get().unwrap(), None);
    }
}
