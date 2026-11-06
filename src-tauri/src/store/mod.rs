//! SQLite session store (KTD1): `rusqlite` with bundled SQLCipher (parity
//! spec 01), WAL mode, foreign keys on. One connection behind a mutex serves the commands
//! and the recording-time chunk writers (U8). A second `Store::open` would run
//! launch recovery and mark the live recording interrupted.

pub mod key;
mod migrations;
mod plaintext_migration;
mod prompts;
mod sessions;
mod settings;

use std::{fmt, path::Path, sync::Mutex};

use rusqlite::Connection;

use key::{apply_key, new_key, plan_open, DbState, Key, KeyStore, LockReason, Plan};

pub use prompts::Prompt;
pub use sessions::{
    now_ms, AudioSource, DiarizationState, InputMode, KeptAudio, NewSession, Session,
    SessionListItem, SessionPatch, SessionStatus, Timings, AUDIO_RETENTION_DAYS,
    AUDIO_RETENTION_MS,
};
pub use settings::{
    RecordingSettings, ACTIVE_PROMPT_ID, PENDING_UPDATE_INSTALL, SKIPPED_UPDATE_VERSION,
    SUMMARY_MODEL_ID,
};

#[derive(Debug)]
pub enum StoreError {
    Sqlite(rusqlite::Error),
    Json(serde_json::Error),
    NotFound(String),
    /// The database was written by a newer build with more migrations.
    SchemaTooNew {
        found: u32,
        known: u32,
    },
    /// The database can't be opened (R7); the app shows the locked screen.
    Locked(LockReason),
    Io(std::io::Error),
    /// Built-in note styles can't be edited or deleted (spec 03 R4).
    BuiltInPrompt,
    EmptyPrompt,
    PromptTooLong,
    /// Unstarring would leave no favourite style (spec 03 R5).
    LastFavourite,
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sqlite(e) => write!(f, "database error: {e}"),
            Self::Json(e) => write!(f, "stored JSON is invalid: {e}"),
            Self::NotFound(what) => write!(f, "{what} not found"),
            Self::SchemaTooNew { found, known } => write!(
                f,
                "database schema version {found} is newer than this app supports ({known})"
            ),
            Self::Locked(reason) => write!(f, "database is locked: {reason:?}"),
            Self::Io(e) => write!(f, "file error: {e}"),
            Self::BuiltInPrompt => {
                f.write_str("Built-in styles can't be changed. Duplicate it to edit.")
            }
            Self::EmptyPrompt => f.write_str("The style's prompt can't be empty."),
            Self::PromptTooLong => write!(
                f,
                "The style's prompt is longer than {} characters.",
                prompts::MAX_PROMPT_CHARS
            ),
            Self::LastFavourite => f.write_str("At least one style must remain starred."),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<rusqlite::Error> for StoreError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Sqlite(e)
    }
}

impl From<std::io::Error> for StoreError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<serde_json::Error> for StoreError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

pub type Result<T> = std::result::Result<T, StoreError>;

pub struct Store {
    conn: Mutex<Connection>,
}

impl Store {
    /// Opens (creating if needed) the encrypted database at `path` with the
    /// key from `keys`, following the R5 table: migrates a prototype plaintext
    /// DB first, never overwrites a DB it can't decrypt. Then applies pending
    /// migrations and marks sessions a crash left mid-capture as failed.
    pub fn open(path: &Path, keys: &dyn KeyStore) -> Result<Self> {
        let locked = |reason| {
            move |e: String| {
                eprintln!("store locked ({reason:?}): {e}");
                StoreError::Locked(reason)
            }
        };
        let finished_migration =
            plaintext_migration::recover(path).map_err(locked(LockReason::MigrationFailed))?;
        let stored = keys.get().map_err(locked(LockReason::KeychainDenied))?;
        let state = DbState::of(path)?;
        if state == DbState::Plaintext {
            // Before a key is stored: never migrate with a build that can't encrypt.
            // The provider pragmas are empty on an unkeyed connection, so key the probe.
            let probe = Connection::open_in_memory()?;
            apply_key(&probe, &[0; 32])?;
            check_sqlcipher(&probe)?;
        }
        let mut key = match plan_open(state, stored.is_some()) {
            Plan::Locked(reason) => return Err(StoreError::Locked(reason)),
            Plan::CreateWithNewKey | Plan::Migrate { new_key: true } => store_new_key(keys)?,
            Plan::CreateWithStoredKey | Plan::Open | Plan::Migrate { new_key: false } => {
                stored.expect("plan needs a stored key")
            }
        };
        if state == DbState::Plaintext {
            plaintext_migration::migrate(path, &key, migrations::known())
                .map_err(locked(LockReason::MigrationFailed))?;
        }
        let conn = open_keyed(path, &key);
        key.fill(0); // best effort; copies made earlier are not reached
        let store = Self {
            conn: Mutex::new(conn?),
        };
        if finished_migration || state == DbState::Plaintext {
            store.set_setting(ENCRYPTION_NOTICE, "1")?;
        }
        store.recover_interrupted()?;
        Ok(store)
    }

    /// R7 "Start fresh": renames the unreadable DB (and its -wal/-shm) to
    /// `scribular.db.unreadable-<ms>`, then creates a new DB with a new key.
    /// Nothing is deleted. On failure the renames are undone, so the locked
    /// screen still describes the files on disk.
    pub fn start_fresh(path: &Path, keys: &dyn KeyStore) -> Result<Self> {
        let tag = format!(".unreadable-{}", now_ms());
        let mut moved = Vec::new();
        let result = (|| {
            for suffix in ["", "-wal", "-shm"] {
                let from = plaintext_migration::sibling(path, suffix);
                if from.exists() {
                    let to = plaintext_migration::sibling(path, &format!("{tag}{suffix}"));
                    std::fs::rename(&from, &to)?;
                    moved.push((from, to));
                }
            }
            let mut key = store_new_key(keys)?;
            let conn = open_keyed(path, &key);
            key.fill(0);
            conn
        })();
        match result {
            Ok(conn) => Ok(Self {
                conn: Mutex::new(conn),
            }),
            Err(e) => {
                // A half-created new DB would block the rename back.
                for suffix in ["", "-wal", "-shm"] {
                    let new = plaintext_migration::sibling(path, suffix);
                    if moved.iter().any(|(from, _)| *from == new) {
                        let _ = std::fs::remove_file(&new);
                    }
                }
                for (from, to) in moved.iter().rev() {
                    if let Err(re) = std::fs::rename(to, from) {
                        eprintln!("start fresh: undo rename failed: {re}");
                    }
                }
                Err(e)
            }
        }
    }

    /// True once after a prototype DB was encrypted (R9's one-time notice).
    pub fn take_encryption_notice(&self) -> Result<bool> {
        self.with(|c| {
            Ok(c.execute("DELETE FROM settings WHERE key = ?1", [ENCRYPTION_NOTICE])? > 0)
        })
    }

    fn with<T>(&self, f: impl FnOnce(&mut Connection) -> Result<T>) -> Result<T> {
        // A panic while holding the lock can't leave SQLite inconsistent
        // (statements are atomic), so a poisoned lock is safe to reuse.
        let mut conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        f(&mut conn)
    }

    /// `(cipher_version, cipher_provider)` of the linked SQLCipher (AS3).
    pub fn cipher(&self) -> Result<(String, String)> {
        self.with(|c| cipher_info(c))
    }

    /// Schema version after migrations, for tests and diagnostics.
    pub fn schema_version(&self) -> Result<u32> {
        self.with(|c| Ok(c.pragma_query_value(None, "user_version", |r| r.get(0))?))
    }
}

const ENCRYPTION_NOTICE: &str = "encryption_notice";

/// R5 row 1: generate, store, and read back before any DB uses the key.
fn store_new_key(keys: &dyn KeyStore) -> Result<Key> {
    let write_failed = |e: String| {
        eprintln!("store locked (KeychainWriteFailed): {e}");
        StoreError::Locked(LockReason::KeychainWriteFailed)
    };
    let key = new_key().map_err(write_failed)?;
    keys.set(&key).map_err(write_failed)?;
    match keys.get() {
        Ok(Some(back)) if back == key => Ok(key),
        Ok(_) => Err(write_failed("key read back differs".into())),
        Err(e) => Err(write_failed(e)),
    }
}

/// The R12 pragma order: key, SQLCipher check, verify, then the usual setup.
fn open_keyed(path: &Path, key: &Key) -> Result<Connection> {
    let mut conn = Connection::open(path)?;
    apply_key(&conn, key)?;
    // Before any read: plain SQLite would ignore the key and read plaintext.
    check_sqlcipher(&conn)?;
    // The first read decrypts page 1; a wrong key fails here, before any write.
    if let Err(e) = conn.query_row("SELECT count(*) FROM sqlite_master", [], |_| Ok(())) {
        eprintln!("store locked (WrongKey): {e}");
        return Err(StoreError::Locked(LockReason::WrongKey));
    }
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", true)?;
    // Deleted audio pages are zeroed instead of only being marked free.
    conn.pragma_update(None, "secure_delete", true)?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    migrations::migrate(&mut conn)?;
    Ok(conn)
}

fn cipher_info(c: &Connection) -> Result<(String, String)> {
    let get = |pragma: &str| -> Result<String> {
        let v: Option<String> = c
            .query_row(&format!("PRAGMA {pragma}"), [], |r| r.get(0))
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                e => Err(e),
            })?;
        Ok(v.unwrap_or_default())
    };
    Ok((get("cipher_version")?, get("cipher_provider")?))
}

/// R4: plain SQLite ignores `PRAGMA key` and would write plaintext. On macOS
/// the provider must be CommonCrypto, not an OpenSSL picked up at build time.
fn check_sqlcipher(c: &Connection) -> Result<()> {
    let (version, provider) = cipher_info(c)?;
    let ok = !version.is_empty() && (!cfg!(target_os = "macos") || provider == "commoncrypto");
    if !ok {
        eprintln!("store locked (NotSqlcipherBuild): version {version:?}, provider {provider:?}");
        return Err(StoreError::Locked(LockReason::NotSqlcipherBuild));
    }
    Ok(())
}
