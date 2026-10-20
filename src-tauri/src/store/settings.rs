//! Key-value settings.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{InputMode, Result, Store};

/// The prompt new sessions summarize with. Seeded from Electron's
/// `activePromptId`.
pub const ACTIVE_PROMPT_ID: &str = "active_prompt_id";
const INPUT_MODE: &str = "input_mode";
const TARGET_APP: &str = "target_app";

/// What the next recording captures and which prompt summarizes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RecordingSettings {
    pub input_mode: InputMode,
    /// The app whose audio `System` and `MicAndSystem` capture.
    pub target_app: Option<String>,
    pub prompt_id: Option<String>,
}

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

    pub fn recording_settings(&self) -> Result<RecordingSettings> {
        Ok(RecordingSettings {
            input_mode: self
                .get_setting(INPUT_MODE)?
                .and_then(|m| InputMode::parse(&m))
                .unwrap_or(InputMode::Mic),
            target_app: self.get_setting(TARGET_APP)?,
            prompt_id: self.get_setting(ACTIVE_PROMPT_ID)?,
        })
    }

    /// Saves all three at once; an absent app or prompt clears it.
    pub fn set_recording_settings(&self, settings: &RecordingSettings) -> Result<()> {
        self.with(|c| {
            let tx = c.transaction()?;
            insert(&tx, INPUT_MODE, settings.input_mode.as_str())?;
            for (key, value) in [
                (TARGET_APP, &settings.target_app),
                (ACTIVE_PROMPT_ID, &settings.prompt_id),
            ] {
                match value {
                    Some(v) => insert(&tx, key, v)?,
                    None => {
                        tx.execute("DELETE FROM settings WHERE key = ?1", [key])?;
                    }
                }
            }
            tx.commit()?;
            Ok(())
        })
    }
}
