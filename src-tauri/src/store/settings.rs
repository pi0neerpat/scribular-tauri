//! Key-value settings.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{InputMode, Result, Store};

/// The prompt new sessions summarize with. Seeded from Electron's
/// `activePromptId`.
pub const ACTIVE_PROMPT_ID: &str = "active_prompt_id";
pub const MULTI_SPEAKER: &str = "multi_speaker";
const INPUT_MODE: &str = "input_mode";
const TARGET_APP: &str = "target_app";
/// Parity spec 01 R13: "Save session audio", off unless set to "1".
pub(super) const KEEP_AUDIO: &str = "keep_audio";

/// What the next recording captures and which prompt summarizes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RecordingSettings {
    pub input_mode: InputMode,
    /// The app whose audio `System` and `MicAndSystem` capture.
    pub target_app: Option<String>,
    pub prompt_id: Option<String>,
    /// Keep the audio (encrypted, 7 days) after the transcript is saved.
    pub keep_audio: bool,
    pub multi_speaker: bool,
}

pub(super) fn insert(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

pub(super) fn select(conn: &Connection, key: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
            r.get(0)
        })
        .optional()?)
}

impl Store {
    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        self.with(|c| select(c, key))
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.with(|c| insert(c, key, value))
    }

    /// Reads them all under one lock, so a concurrent save is seen whole.
    pub fn recording_settings(&self) -> Result<RecordingSettings> {
        self.with(|c| {
            Ok(RecordingSettings {
                input_mode: select(c, INPUT_MODE)?
                    .and_then(|m| InputMode::parse(&m))
                    .unwrap_or(InputMode::Mic),
                target_app: select(c, TARGET_APP)?,
                prompt_id: select(c, ACTIVE_PROMPT_ID)?,
                keep_audio: select(c, KEEP_AUDIO)?.as_deref() == Some("1"),
                multi_speaker: select(c, MULTI_SPEAKER)?.as_deref() == Some("1"),
            })
        })
    }

    /// Saves them all at once; an absent app or prompt clears it.
    pub fn set_recording_settings(&self, settings: &RecordingSettings) -> Result<()> {
        self.with(|c| {
            let tx = c.transaction()?;
            insert(&tx, INPUT_MODE, settings.input_mode.as_str())?;
            insert(&tx, KEEP_AUDIO, if settings.keep_audio { "1" } else { "0" })?;
            insert(
                &tx,
                MULTI_SPEAKER,
                if settings.multi_speaker { "1" } else { "0" },
            )?;
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
