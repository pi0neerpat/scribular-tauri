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

/// The raw-key form SQLCipher recognises: `x'<64 hex>'` (R2). Callers wrap it
/// in [`Scrubbed`] so the buffer is overwritten when they are done.
pub fn raw_key(key: &Key) -> Scrubbed {
    let mut s = String::with_capacity(KEY_LEN * 2 + 3);
    s.push_str("x'");
    for b in key {
        write!(s, "{b:02x}").unwrap();
    }
    s.push('\'');
    Scrubbed(s)
}

/// `PRAGMA key`, which must be the first statement on the connection.
pub fn apply_key(conn: &rusqlite::Connection, key: &Key) -> rusqlite::Result<()> {
    let raw = raw_key(key);
    let stmt = Scrubbed(format!("PRAGMA key = \"{}\";", raw.0));
    conn.execute_batch(&stmt.0)
}

/// An ASCII secret overwritten on drop. Best effort only: earlier
/// reallocations or copies made by SQLite are not reached.
pub struct Scrubbed(pub String);

impl Drop for Scrubbed {
    fn drop(&mut self) {
        // SAFETY: '0' is ASCII, so the string stays valid UTF-8.
        unsafe { self.0.as_bytes_mut() }.fill(b'0');
    }
}

/// 32 bytes from the OS CSPRNG.
pub fn new_key() -> Result<Key, String> {
    let mut key = [0u8; KEY_LEN];
    #[cfg(target_os = "macos")]
    security_framework::random::SecRandom::default()
        .copy_bytes(&mut key)
        .map_err(|e| format!("SecRandomCopyBytes: {e}"))?;
    #[cfg(not(target_os = "macos"))]
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut key))
        .map_err(|e| format!("/dev/urandom: {e}"))?;
    Ok(key)
}

/// The key as a generic password in the login Keychain (R3, option A in
/// plans/DEPENDENCY_REVIEW_ENCRYPTION.md). Spec 04 reuses it for the licence.
#[cfg(target_os = "macos")]
pub struct KeychainKeyStore {
    service: String,
    account: String,
}

#[cfg(target_os = "macos")]
impl KeychainKeyStore {
    pub fn new(service: &str, account: &str) -> Self {
        Self {
            service: service.into(),
            account: account.into(),
        }
    }

    fn options(&self) -> security_framework::passwords::PasswordOptions {
        security_framework::passwords::PasswordOptions::new_generic_password(
            &self.service,
            &self.account,
        )
    }
}

#[cfg(target_os = "macos")]
impl KeyStore for KeychainKeyStore {
    fn get(&self) -> Result<Option<Key>, String> {
        const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;
        match security_framework::passwords::generic_password(self.options()) {
            Ok(mut bytes) => {
                let key = Key::try_from(bytes.as_slice())
                    .map_err(|_| format!("stored key has {} bytes", bytes.len()));
                bytes.fill(0);
                key.map(Some)
            }
            Err(e) if e.code() == ERR_SEC_ITEM_NOT_FOUND => Ok(None),
            Err(e) => Err(format!("Keychain read failed ({})", e.code())),
        }
    }

    fn set(&self, key: &Key) -> Result<(), String> {
        security_framework::passwords::set_generic_password_options(key, self.options())
            .map_err(|e| format!("Keychain write failed ({})", e.code()))
    }
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
    fn raw_key_is_hex() {
        let mut key = [0u8; KEY_LEN];
        key[0] = 0xab;
        key[31] = 0x01;
        let p = raw_key(&key);
        assert_eq!(p.0.len(), 64 + 3);
        assert!(p.0.starts_with("x'ab00"));
        assert!(p.0.ends_with("01'"));
    }

    #[test]
    fn new_keys_differ() {
        assert_ne!(new_key().unwrap(), new_key().unwrap());
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
