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
pub const NOTIFY_SUMMARY_COMPLETE: &str = "notify_summary_complete";
pub const SHOW_BACKGROUND_ANIMATION: &str = "show_background_animation";
pub const CHECK_FOR_UPDATES_AUTOMATICALLY: &str = "check_for_updates_automatically";
pub const ONBOARDING_COMPLETED: &str = "onboarding_completed";
pub(super) const FIRST_NOTE_TIP_DISMISSED: &str = "first_note_tip_dismissed";
pub const APP_AUDIO_EXPLAINER_SHOWN: &str = "app_audio_explainer_shown";
pub const SAMPLES_SEEDED: &str = "samples_seeded";
const INPUT_MODE: &str = "input_mode";
const TARGET_APP: &str = "target_app";
const RECORD_LOCATION: &str = "record_location";
const RECORD_TARGET_APP: &str = "record_target_app";
const GLOBAL_SHORTCUTS_ENABLED: &str = "global_shortcuts_enabled";
const TOGGLE_RECORDING_SHORTCUT: &str = "toggle_recording_shortcut";
pub const DEFAULT_TOGGLE_RECORDING: &str = "control+alt+super+KeyR";
/// Parity spec 01 R13: "Save session audio", off unless set to "1".
pub(super) const KEEP_AUDIO: &str = "keep_audio";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub notify_summary_complete: bool,
    pub show_background_animation: bool,
    pub check_for_updates_automatically: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            notify_summary_complete: false,
            show_background_animation: true,
            check_for_updates_automatically: true,
        }
    }
}

/// What the next recording captures and which prompt summarizes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RecordingSettings {
    pub input_mode: InputMode,
    /// The app whose audio `MicAndSystem` captures.
    pub target_app: Option<String>,
    pub prompt_id: Option<String>,
    /// Keep the audio (encrypted, 7 days) after the transcript is saved.
    pub keep_audio: bool,
    pub multi_speaker: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutSettings {
    pub global_shortcuts_enabled: bool,
    pub toggle_recording: Option<String>,
}

impl Default for ShortcutSettings {
    fn default() -> Self {
        Self {
            global_shortcuts_enabled: false,
            toggle_recording: Some(DEFAULT_TOGGLE_RECORDING.into()),
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum RecordLocation {
    #[default]
    InRoom,
    OnCall,
}

impl RecordLocation {
    pub fn input_mode(self) -> InputMode {
        match self {
            Self::InRoom => InputMode::Mic,
            Self::OnCall => InputMode::MicAndSystem,
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RecordingSource {
    pub location: RecordLocation,
    pub target_app: Option<String>,
}

pub(crate) fn valid_target_app(app: &str) -> bool {
    !app.trim().is_empty() && !app.contains('\0')
}

fn recording_source(c: &Connection) -> Result<RecordingSource> {
    let location = select(c, RECORD_LOCATION)?;
    let on_call = match location.as_deref() {
        Some(location) => location == "onCall",
        None => select(c, INPUT_MODE)?.as_deref() == Some(InputMode::MicAndSystem.as_str()),
    };
    let target_app = select(
        c,
        if location.is_some() {
            RECORD_TARGET_APP
        } else {
            TARGET_APP
        },
    )?
    .filter(|app| valid_target_app(app));
    Ok(RecordingSource {
        location: if on_call && target_app.is_some() {
            RecordLocation::OnCall
        } else {
            RecordLocation::InRoom
        },
        target_app,
    })
}

fn save_source(c: &Connection, source: &RecordingSource) -> Result<()> {
    let app = source.target_app.as_deref();
    if app.map_or(source.location == RecordLocation::OnCall, |app| {
        !valid_target_app(app)
    }) {
        return Err(super::StoreError::InvalidRecordingSource);
    }
    insert(
        c,
        RECORD_LOCATION,
        match source.location {
            RecordLocation::InRoom => "inRoom",
            RecordLocation::OnCall => "onCall",
        },
    )?;
    match app {
        Some(app) => insert(c, RECORD_TARGET_APP, app)?,
        None => {
            c.execute("DELETE FROM settings WHERE key = ?1", [RECORD_TARGET_APP])?;
        }
    }
    Ok(())
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
    pub fn shortcut_settings(&self) -> Result<ShortcutSettings> {
        self.with(|c| {
            Ok(ShortcutSettings {
                global_shortcuts_enabled: select(c, GLOBAL_SHORTCUTS_ENABLED)?.as_deref()
                    == Some("1"),
                toggle_recording: select(c, TOGGLE_RECORDING_SHORTCUT)?
                    .map(|s| if s.is_empty() { None } else { Some(s) })
                    .unwrap_or_else(|| Some(DEFAULT_TOGGLE_RECORDING.into())),
            })
        })
    }

    pub fn set_shortcut_settings(&self, settings: &ShortcutSettings) -> Result<()> {
        self.with(|c| {
            let tx = c.transaction()?;
            insert(
                &tx,
                GLOBAL_SHORTCUTS_ENABLED,
                if settings.global_shortcuts_enabled {
                    "1"
                } else {
                    "0"
                },
            )?;
            insert(
                &tx,
                TOGGLE_RECORDING_SHORTCUT,
                settings.toggle_recording.as_deref().unwrap_or(""),
            )?;
            tx.commit()?;
            Ok(())
        })
    }

    pub fn recording_source(&self) -> Result<RecordingSource> {
        self.with(|c| recording_source(c))
    }

    pub fn set_recording_source(&self, source: &RecordingSource) -> Result<()> {
        self.with(|c| {
            let tx = c.transaction()?;
            save_source(&tx, source)?;
            tx.commit()?;
            Ok(())
        })
    }

    pub fn app_settings(&self) -> Result<AppSettings> {
        self.with(|c| {
            Ok(AppSettings {
                notify_summary_complete: select(c, NOTIFY_SUMMARY_COMPLETE)?.as_deref()
                    == Some("true"),
                show_background_animation: select(c, SHOW_BACKGROUND_ANIMATION)?.as_deref()
                    != Some("false"),
                check_for_updates_automatically: select(c, CHECK_FOR_UPDATES_AUTOMATICALLY)?
                    .as_deref()
                    != Some("false"),
            })
        })
    }

    pub fn set_app_settings(&self, settings: AppSettings) -> Result<()> {
        self.with(|c| {
            let tx = c.transaction()?;
            insert(
                &tx,
                NOTIFY_SUMMARY_COMPLETE,
                if settings.notify_summary_complete {
                    "true"
                } else {
                    "false"
                },
            )?;
            insert(
                &tx,
                SHOW_BACKGROUND_ANIMATION,
                if settings.show_background_animation {
                    "true"
                } else {
                    "false"
                },
            )?;
            insert(
                &tx,
                CHECK_FOR_UPDATES_AUTOMATICALLY,
                if settings.check_for_updates_automatically {
                    "true"
                } else {
                    "false"
                },
            )?;
            tx.commit()?;
            Ok(())
        })
    }

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

    pub fn first_note_tip_dismissed(&self) -> Result<bool> {
        Ok(self.get_setting(FIRST_NOTE_TIP_DISMISSED)?.as_deref() == Some("1"))
    }

    pub fn dismiss_first_note_tip(&self) -> Result<()> {
        self.set_setting(FIRST_NOTE_TIP_DISMISSED, "1")
    }

    pub fn app_audio_explainer_shown(&self) -> Result<bool> {
        Ok(self.get_setting(APP_AUDIO_EXPLAINER_SHOWN)?.as_deref() == Some("1"))
    }

    pub fn mark_app_audio_explainer_shown(&self) -> Result<()> {
        self.set_setting(APP_AUDIO_EXPLAINER_SHOWN, "1")
    }

    /// Reads them all under one lock, so a concurrent save is seen whole.
    pub fn recording_settings(&self) -> Result<RecordingSettings> {
        self.with(|c| {
            let source = recording_source(c)?;
            Ok(RecordingSettings {
                input_mode: source.location.input_mode(),
                target_app: source.target_app,
                prompt_id: select(c, ACTIVE_PROMPT_ID)?,
                keep_audio: select(c, KEEP_AUDIO)?.as_deref() == Some("1"),
                multi_speaker: select(c, MULTI_SPEAKER)?.as_deref() == Some("1"),
            })
        })
    }

    /// Saves them all at once; an absent app clears it in mic mode, an absent prompt
    /// keeps the active one. Unknown ids are rejected.
    pub fn set_recording_settings(&self, settings: &RecordingSettings) -> Result<()> {
        self.with(|c| {
            let tx = c.transaction()?;
            save_source(
                &tx,
                &RecordingSource {
                    location: match settings.input_mode {
                        InputMode::Mic => RecordLocation::InRoom,
                        InputMode::MicAndSystem => RecordLocation::OnCall,
                        InputMode::System => return Err(super::StoreError::InvalidRecordingSource),
                    },
                    target_app: settings.target_app.clone(),
                },
            )?;
            insert(&tx, KEEP_AUDIO, if settings.keep_audio { "1" } else { "0" })?;
            insert(
                &tx,
                MULTI_SPEAKER,
                if settings.multi_speaker { "1" } else { "0" },
            )?;
            // One validated writer for the active style (spec 03 R7); an
            // absent id keeps it, since some style is always active (R6).
            if let Some(id) = &settings.prompt_id {
                super::prompts::set_active(&tx, id)?;
            }
            tx.commit()?;
            Ok(())
        })
    }

    pub fn set_recording_preferences(&self, keep_audio: bool, multi_speaker: bool) -> Result<()> {
        self.with(|c| {
            let tx = c.transaction()?;
            insert(&tx, KEEP_AUDIO, if keep_audio { "1" } else { "0" })?;
            insert(&tx, MULTI_SPEAKER, if multi_speaker { "1" } else { "0" })?;
            tx.commit()?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod shortcut_tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn shortcut_settings_round_trip_and_clear() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
            [],
        )
        .unwrap();
        let store = Store {
            conn: Mutex::new(conn),
        };
        assert_eq!(
            store.shortcut_settings().unwrap(),
            ShortcutSettings::default()
        );
        let settings = ShortcutSettings {
            global_shortcuts_enabled: true,
            toggle_recording: None,
        };
        store.set_shortcut_settings(&settings).unwrap();
        assert_eq!(store.shortcut_settings().unwrap(), settings);
        store
            .set_shortcut_settings(&ShortcutSettings::default())
            .unwrap();
        assert_eq!(
            store.shortcut_settings().unwrap(),
            ShortcutSettings::default()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        diag::telemetry,
        store::{key::MemoryKeyStore, NewSession},
    };

    #[test]
    fn app_audio_flag_is_idempotent_and_failed_write_preserves_existing_data() {
        let path =
            std::env::temp_dir().join(format!("scribular-app-audio-{}.db", std::process::id()));
        let keys = MemoryKeyStore::default();
        let store = Store::open(&path, &keys).unwrap();
        let session = store
            .create_session(&NewSession {
                title: "keep".into(),
                input_mode: InputMode::Mic,
                prompt_id: None,
                model_id: None,
            })
            .unwrap();
        store.set_setting("keep_setting", "keep_value").unwrap();
        assert!(!store.app_audio_explainer_shown().unwrap());
        store.with(|conn| {
            conn.execute_batch("CREATE TEMP TRIGGER reject_app_audio BEFORE INSERT ON settings
                WHEN NEW.key = 'app_audio_explainer_shown' BEGIN SELECT RAISE(ABORT, 'test write failure'); END;")?;
            Ok(())
        }).unwrap();
        assert!(store.mark_app_audio_explainer_shown().is_err());
        assert!(!store.app_audio_explainer_shown().unwrap());
        drop(store);
        let store = Store::open(&path, &keys).unwrap();
        store.mark_app_audio_explainer_shown().unwrap();
        store.mark_app_audio_explainer_shown().unwrap();
        assert!(store.app_audio_explainer_shown().unwrap());
        assert_eq!(
            store.get_setting("keep_setting").unwrap().as_deref(),
            Some("keep_value")
        );
        assert!(store.get_session(&session.id).unwrap().is_some());
        drop(store);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn failed_welcome_writes_leave_consent_off_and_welcome_unseen_on_reopen() {
        for key in [telemetry::TELEMETRY_ENABLED, ONBOARDING_COMPLETED] {
            let path = std::env::temp_dir()
                .join(format!("scribular-welcome-{}-{key}.db", std::process::id()));
            let keys = MemoryKeyStore::default();
            let store = Store::open(&path, &keys).unwrap();
            assert!(!store.onboarding_completed().unwrap());
            store
                .with(|conn| {
                    conn.execute_batch(&format!(
                        "CREATE TEMP TRIGGER reject_welcome_write BEFORE INSERT ON settings
                         WHEN NEW.key = '{key}' BEGIN SELECT RAISE(ABORT, 'test write failure'); END;"
                    ))?;
                    Ok(())
                })
                .unwrap();
            let saved = telemetry::set_enabled(&store, false)
                .and_then(|()| store.complete_onboarding().map_err(|e| e.to_string()));
            assert!(saved.is_err());
            drop(store);

            let store = Store::open(&path, &keys).unwrap();
            assert!(!store.onboarding_completed().unwrap());
            assert!(!telemetry::send_allowed(&store));
            telemetry::set_enabled(&store, false).unwrap();
            store.complete_onboarding().unwrap();
            drop(store);

            let store = Store::open(&path, &keys).unwrap();
            assert!(store.onboarding_completed().unwrap());
            assert_eq!(
                store
                    .get_setting(telemetry::TELEMETRY_ENABLED)
                    .unwrap()
                    .as_deref(),
                Some("0")
            );
            drop(store);
            std::fs::remove_file(path).unwrap();
        }
    }
}
