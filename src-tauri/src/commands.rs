//! Tauri commands and the IPC types they exchange with the webview.
//!
//! Every type that crosses IPC derives `TS` and is listed in
//! [`export_ipc_types`]; `cargo test` writes them to `src/lib/types/`, and
//! those generated files are committed.

use std::{
    path::Path,
    sync::{atomic::AtomicBool, Arc},
};

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use ts_rs::TS;

use crate::audio::{AudioApp, Privacy};
use crate::jobs::{JobEvents, JobQueue, Recorder, SessionProgress, SessionSegments, SummaryDelta};
use crate::models::{ModelManager, ModelProgress, ModelSpec};
use crate::store::{
    InputMode, Prompt, RecordingSettings, Session, SessionListItem, SessionPatch, Store,
};
use crate::stt::{read_wav, TranscriberLoader, TranscriptSegment};

/// Event the core emits once a second, as the example of a typed event.
pub const HEARTBEAT_EVENT: &str = "app://heartbeat";

/// Every event the core emits; each needs a `listen` wrapper in api.ts.
pub const EVENTS: &[&str] = &[
    HEARTBEAT_EVENT,
    MODELS_PROGRESS_EVENT,
    SESSION_PROGRESS_EVENT,
    SUMMARY_DELTA_EVENT,
    SESSION_SEGMENTS_EVENT,
];

/// Download and verification progress, one [`ModelProgress`] per change.
pub const MODELS_PROGRESS_EVENT: &str = "models://progress";

/// Job progress, one [`SessionProgress`] per step or state change.
pub const SESSION_PROGRESS_EVENT: &str = "session://progress";

/// Summary text as it decodes, one [`SummaryDelta`] per token.
pub const SUMMARY_DELTA_EVENT: &str = "session://summary-delta";

/// Transcript segments as a recording transcribes, one [`SessionSegments`]
/// per window that had speech.
pub const SESSION_SEGMENTS_EVENT: &str = "session://segments";

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AppVersion {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Heartbeat {
    pub seq: u32,
}

/// Exports every IPC root type, plus the types they reference, into
/// `cfg.out_dir()`.
#[cfg(test)]
pub(crate) fn export_ipc_types(cfg: &ts_rs::Config) -> Result<(), ts_rs::ExportError> {
    AppVersion::export_all(cfg)?;
    Heartbeat::export_all(cfg)?;
    SessionListItem::export_all(cfg)?;
    Session::export_all(cfg)?;
    SessionPatch::export_all(cfg)?;
    Prompt::export_all(cfg)?;
    ModelSpec::export_all(cfg)?;
    ModelProgress::export_all(cfg)?;
    SessionProgress::export_all(cfg)?;
    SummaryDelta::export_all(cfg)?;
    SessionSegments::export_all(cfg)?;
    TranscriptSegment::export_all(cfg)?;
    AudioApp::export_all(cfg)?;
    RecordingSettings::export_all(cfg)?;
    Privacy::export_all(cfg)?;
    Ok(())
}

#[tauri::command]
pub fn app_version() -> AppVersion {
    AppVersion {
        name: env!("CARGO_PKG_NAME").to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

// Commands return `Result<_, String>`: the webview only shows the message.

#[tauri::command]
pub fn list_sessions(store: State<Arc<Store>>) -> Result<Vec<SessionListItem>, String> {
    store.list_sessions().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_session(store: State<Arc<Store>>, id: String) -> Result<Option<Session>, String> {
    store.get_session(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_session(
    store: State<Arc<Store>>,
    id: String,
    patch: SessionPatch,
) -> Result<Session, String> {
    store.update_session(&id, &patch).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_session(store: State<Arc<Store>>, id: String) -> Result<bool, String> {
    store.delete_session(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_prompts(store: State<Arc<Store>>) -> Result<Vec<Prompt>, String> {
    store.list_prompts().map_err(|e| e.to_string())
}

/// The pinned models with their license terms, in download order.
#[tauri::command]
pub fn list_models(models: State<Arc<ModelManager>>) -> Vec<ModelSpec> {
    models.specs().to_vec()
}

#[tauri::command]
pub fn models_status(models: State<Arc<ModelManager>>) -> Vec<ModelProgress> {
    models.status()
}

/// Starts downloading any missing model in the background and returns at
/// once; progress arrives on [`MODELS_PROGRESS_EVENT`].
#[tauri::command]
pub fn download_models(app: AppHandle, models: State<Arc<ModelManager>>) {
    let models = Arc::clone(&models);
    tauri::async_runtime::spawn(async move {
        models
            .ensure_all(&|p| {
                let _ = app.emit(MODELS_PROGRESS_EVENT, p);
            })
            .await;
    });
}

/// Queues a fresh summary; progress arrives on [`SESSION_PROGRESS_EVENT`].
#[tauri::command]
pub fn regenerate_summary(
    jobs: State<Arc<JobQueue>>,
    session_id: String,
    prompt_id: String,
) -> Result<(), String> {
    jobs.regenerate(&session_id, &prompt_id)
        .map_err(|e| e.to_string())
}

/// Stops the session's transcription or summary. Returns whether it had
/// either.
#[tauri::command]
pub fn cancel_job(recorder: State<Arc<Recorder>>, session_id: String) -> bool {
    recorder.cancel(&session_id)
}

#[tauri::command]
pub fn get_recording_settings(store: State<Arc<Store>>) -> Result<RecordingSettings, String> {
    store.recording_settings().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_recording_settings(
    store: State<Arc<Store>>,
    settings: RecordingSettings,
) -> Result<(), String> {
    store
        .set_recording_settings(&settings)
        .map_err(|e| e.to_string())
}

/// Apps playing audio now. Runs AudioCapCLI, so off the main thread.
#[tauri::command]
pub async fn list_audio_apps(recorder: State<'_, Arc<Recorder>>) -> Result<Vec<AudioApp>, String> {
    let recorder = Arc::clone(&recorder);
    tauri::async_runtime::spawn_blocking(move || {
        recorder
            .system_audio()
            .list_apps()
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Starts recording a new session; `app_id` is required unless `mode` is
/// mic only. A refused permission fails with "<source> permission denied";
/// [`open_privacy_settings`] then takes the user to the pane.
#[tauri::command]
pub async fn start_recording(
    recorder: State<'_, Arc<Recorder>>,
    mode: InputMode,
    app_id: Option<String>,
    title: String,
) -> Result<Session, String> {
    let recorder = Arc::clone(&recorder);
    tauri::async_runtime::spawn_blocking(move || {
        recorder
            .start(mode, app_id.as_deref(), &title)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Stops recording and starts transcription; returns the session id.
#[tauri::command]
pub async fn stop_recording(recorder: State<'_, Arc<Recorder>>) -> Result<String, String> {
    let recorder = Arc::clone(&recorder);
    tauri::async_runtime::spawn_blocking(move || recorder.stop().map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}

/// Opens the System Settings pane that grants `pane`'s permission.
#[tauri::command]
pub fn open_privacy_settings(pane: Privacy) -> Result<(), String> {
    std::process::Command::new("/usr/bin/open")
        .arg(pane.settings_url())
        .status()
        .map_err(|e| e.to_string())
        .and_then(|s| {
            s.success()
                .then_some(())
                .ok_or_else(|| "System Settings did not open".to_string())
        })
}

/// The id of the session being recorded, if any.
#[tauri::command]
pub fn current_recording(recorder: State<Arc<Recorder>>) -> Option<String> {
    recorder.current()
}

/// Development only (M4): transcribes a 16-bit, 16 kHz mono WAV file.
#[tauri::command]
pub async fn transcribe_file(
    stt: State<'_, Arc<dyn TranscriberLoader>>,
    path: String,
) -> Result<Vec<TranscriptSegment>, String> {
    let stt = Arc::clone(&stt);
    tauri::async_runtime::spawn_blocking(move || {
        let samples = read_wav(Path::new(&path)).map_err(|e| e.to_string())?;
        stt.load()
            .and_then(|t| t.transcribe(&samples, &AtomicBool::new(false)))
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Emits job events to the webview.
pub struct TauriJobEvents(pub AppHandle);

impl JobEvents for TauriJobEvents {
    fn progress(&self, progress: SessionProgress) {
        let _ = self.0.emit(SESSION_PROGRESS_EVENT, progress);
    }

    fn summary_delta(&self, delta: SummaryDelta) {
        let _ = self.0.emit(SUMMARY_DELTA_EVENT, delta);
    }

    fn segments(&self, segments: SessionSegments) {
        let _ = self.0.emit(SESSION_SEGMENTS_EVENT, segments);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regenerates `src/lib/types/`. ts-rs exports into a temp dir and this
    /// copies the result over, because ts-rs's own writer calls
    /// `F_FULLFSYNC`, which the VirtioFS share in the dev VM rejects.
    /// Generated files that no longer correspond to a type are deleted.
    #[test]
    fn export_bindings() {
        use std::{collections::BTreeMap, fs, path::Path};

        let tmp = std::env::temp_dir().join(format!("scribular-ts-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        export_ipc_types(&ts_rs::Config::new().with_out_dir(&tmp)).unwrap();

        let read_ts = |dir: &Path| -> BTreeMap<String, String> {
            fs::read_dir(dir)
                .into_iter()
                .flatten()
                .map(|e| e.unwrap().path())
                .filter(|p| p.extension().is_some_and(|x| x == "ts"))
                .map(|p| {
                    let name = p.file_name().unwrap().to_string_lossy().into_owned();
                    (name, fs::read_to_string(&p).unwrap())
                })
                .collect()
        };
        let generated = read_ts(&tmp);
        fs::remove_dir_all(&tmp).unwrap();
        assert!(!generated.is_empty());

        let dest = Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/lib/types");
        fs::create_dir_all(&dest).unwrap();
        for (name, old) in read_ts(&dest) {
            match generated.get(&name) {
                Some(new) if *new == old => {}
                Some(new) => fs::write(dest.join(&name), new).unwrap(),
                None => fs::remove_file(dest.join(&name)).unwrap(),
            }
        }
        for (name, new) in &generated {
            if !dest.join(name).exists() {
                fs::write(dest.join(name), new).unwrap();
            }
        }
    }

    #[test]
    fn app_version_returns_crate_version() {
        let v = app_version();
        assert_eq!(v.name, "scribular");
        assert_eq!(v.version, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn app_version_serializes_camel_case() {
        let json = serde_json::to_value(app_version()).unwrap();
        assert!(json.get("version").is_some());
        assert!(json.get("name").is_some());
    }
}
