//! Key-value settings.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{InputMode, Result, Store};

/// The prompt new sessions summarize with. Seeded from Electron's
/// `activePromptId`.
pub const ACTIVE_PROMPT_ID: &str = "active_prompt_id";
pub const MULTI_SPEAKER: &str = "multi_speaker";
/// The manifest id of the summary model (parity spec 03 R18).
pub const SUMMARY_MODEL_ID: &str = "summary_model_id";
pub const PENDING_UPDATE_INSTALL: &str = "pending_update_install";
pub const SKIPPED_UPDATE_VERSION: &str = "skipped_update_version";
pub const ONBOARDING_COMPLETED: &str = "onboarding_completed";
pub const SAMPLES_SEEDED: &str = "samples_seeded";
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

    pub fn delete_setting(&self, key: &str) -> Result<()> {
        self.with(|c| {
            c.execute("DELETE FROM settings WHERE key = ?1", [key])?;
            Ok(())
        })
    }

    pub fn complete_onboarding(&self) -> Result<()> {
        self.set_setting(ONBOARDING_COMPLETED, "1")
    }

    pub fn onboarding_completed(&self) -> Result<bool> {
        Ok(self.get_setting(ONBOARDING_COMPLETED)?.as_deref() == Some("1"))
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

    /// Saves them all at once; an absent app clears it, an absent prompt
    /// keeps the active one. Unknown ids are rejected and unstarred styles
    /// are starred, matching `set_active_prompt`.
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
            match &settings.target_app {
                Some(app) => insert(&tx, TARGET_APP, app)?,
                None => {
                    tx.execute("DELETE FROM settings WHERE key = ?1", [TARGET_APP])?;
                }
            }
            // One validated writer for the active style (spec 03 R7); an
            // absent id keeps it, since some style is always active (R6).
            if let Some(id) = &settings.prompt_id {
                super::prompts::set_active(&tx, id)?;
            }
            tx.commit()?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use std::sync::Mutex;

    fn store() -> Store {
        let mut conn = Connection::open_in_memory().unwrap();
        super::super::migrations::migrate(&mut conn).unwrap();
        Store {
            conn: Mutex::new(conn),
        }
    }

    #[test]
    fn onboarding_choice_persists() {
        let store = store();
        assert!(!store.onboarding_completed().unwrap());
        store.complete_onboarding().unwrap();
        assert!(store.onboarding_completed().unwrap());
    }
}
