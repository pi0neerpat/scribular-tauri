//! SQLite session store (KTD1): plain `rusqlite` with bundled SQLite, WAL
//! mode, foreign keys on. One connection behind a mutex serves the commands
//! and the recording-time chunk writers (U8). A second `Store::open` would run
//! launch recovery and mark the live recording interrupted.

mod migrations;
mod prompts;
mod sessions;
mod settings;

use std::{fmt, path::Path, sync::Mutex};

use rusqlite::Connection;

pub use prompts::Prompt;
pub use sessions::{
    AudioSource, InputMode, NewSession, Session, SessionListItem, SessionPatch, SessionStatus,
    Timings,
};
pub use settings::{RecordingSettings, ACTIVE_PROMPT_ID};

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
        }
    }
}

impl std::error::Error for StoreError {}

impl From<rusqlite::Error> for StoreError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Sqlite(e)
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
    /// Opens (creating if needed) the database at `path`, applies pending
    /// migrations, and marks sessions a crash left mid-capture as failed.
    pub fn open(path: &Path) -> Result<Self> {
        let mut conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", true)?;
        // Deleted audio pages are zeroed instead of only being marked free.
        conn.pragma_update(None, "secure_delete", true)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        migrations::migrate(&mut conn)?;
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.recover_interrupted()?;
        Ok(store)
    }

    fn with<T>(&self, f: impl FnOnce(&mut Connection) -> Result<T>) -> Result<T> {
        // A panic while holding the lock can't leave SQLite inconsistent
        // (statements are atomic), so a poisoned lock is safe to reuse.
        let mut conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        f(&mut conn)
    }

    /// Schema version after migrations, for tests and diagnostics.
    pub fn schema_version(&self) -> Result<u32> {
        self.with(|c| Ok(c.pragma_query_value(None, "user_version", |r| r.get(0))?))
    }
}
