//! Key-value settings.

use rusqlite::{params, Connection, OptionalExtension};

use super::{Result, Store};

/// The prompt new sessions summarize with. Seeded from Electron's
/// `activePromptId`.
pub const ACTIVE_PROMPT_ID: &str = "active_prompt_id";

pub(super) fn insert(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

impl Store {
    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        self.with(|c| {
            Ok(
                c.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
                    r.get(0)
                })
                .optional()?,
            )
        })
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.with(|c| insert(c, key, value))
    }
}
