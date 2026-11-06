//! Tauri commands and the IPC types they exchange with the webview.
//!
//! Every type that crosses IPC derives `TS` and is listed in
//! [`export_ipc_types`]; `cargo test` writes them to `src/lib/types/`, and
//! those generated files are committed.

use std::{
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Arc},
};

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use ts_rs::TS;

use crate::audio::{AudioApp, Privacy};
use crate::jobs::{JobEvents, JobQueue, Recorder, SessionProgress, SessionSegments, SummaryDelta};
use crate::license::{AccessState, ActivationResult, Licensing};
use crate::models::{
    ModelKind, ModelManager, ModelProgress, ModelSpec, ModelState, SPEAKER_CAMPPLUS,
};
use crate::store::key::{KeyStore, LockReason};
use crate::store::{
    now_ms, InputMode, KeptAudio, Prompt, RecordingSettings, Session, SessionListItem,
    SessionPatch, Store, SUMMARY_MODEL_ID,
};
use crate::stt::{read_wav, TranscriberLoader, TranscriptSegment};

/// Event the core emits once a second, as the example of a typed event.
pub const HEARTBEAT_EVENT: &str = "app://heartbeat";

/// Every event the core emits; each needs a `listen` wrapper in api.ts.
pub const EVENTS: &[&str] = &[
    HEARTBEAT_EVENT,
    MODELS_PROGRESS_EVENT,
    SUMMARY_MODEL_CHANGED_EVENT,
    SESSION_PROGRESS_EVENT,
    SUMMARY_DELTA_EVENT,
    SESSION_SEGMENTS_EVENT,
    LICENSE_CHANGED_EVENT,
    UPDATE_STATE_EVENT,
];

/// The new [`AccessState`] after activation, removal, re-validation or a day
/// rollover (spec 04 R12).
pub const LICENSE_CHANGED_EVENT: &str = "license://changed";

pub const UPDATE_STATE_EVENT: &str = "update://state";

/// Download and verification progress, one [`ModelProgress`] per change.
pub const MODELS_PROGRESS_EVENT: &str = "models://progress";

/// The selected summary model changed or reverted after a failed download.
pub const SUMMARY_MODEL_CHANGED_EVENT: &str = "summary-model://changed";

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
    PromptList::export_all(cfg)?;
    SummaryModel::export_all(cfg)?;
    ModelSpec::export_all(cfg)?;
    ModelProgress::export_all(cfg)?;
    SessionProgress::export_all(cfg)?;
    SummaryDelta::export_all(cfg)?;
    SessionSegments::export_all(cfg)?;
    TranscriptSegment::export_all(cfg)?;
    AudioApp::export_all(cfg)?;
    RecordingSettings::export_all(cfg)?;
    Privacy::export_all(cfg)?;
    PlaybackError::export_all(cfg)?;
    LockReason::export_all(cfg)?;
    AccessState::export_all(cfg)?;
    ActivationResult::export_all(cfg)?;
    crate::license::Rejection::export_all(cfg)?;
    crate::update::UpdateState::export_all(cfg)?;
    Ok(())
}

#[tauri::command]
pub fn app_version() -> AppVersion {
    AppVersion {
        name: env!("CARGO_PKG_NAME").to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

#[tauri::command]
pub fn check_for_updates(updates: State<'_, Arc<crate::update::UpdateManager>>) {
    let updates = Arc::clone(&updates);
    std::thread::spawn(move || updates.check());
}

#[tauri::command]
pub fn get_update_state(
    updates: State<'_, Arc<crate::update::UpdateManager>>,
) -> crate::update::UpdateState {
    updates.state()
}

#[tauri::command]
pub fn install_update(updates: State<'_, Arc<crate::update::UpdateManager>>) -> Result<(), String> {
    updates.install()
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

/// Every style after a change, so the UI never merges client-side
/// (parity spec 03 R14).
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PromptList {
    pub prompts: Vec<Prompt>,
    pub active_id: Option<String>,
    /// The style the call created, for create and duplicate.
    pub created_id: Option<String>,
}

fn prompt_list(store: &Store, created_id: Option<String>) -> Result<PromptList, String> {
    Ok(PromptList {
        prompts: store.list_prompts().map_err(|e| e.to_string())?,
        active_id: store.get_active_prompt_id().map_err(|e| e.to_string())?,
        created_id,
    })
}

#[tauri::command]
pub fn get_prompt_list(store: State<Arc<Store>>) -> Result<PromptList, String> {
    prompt_list(&store, None)
}

#[tauri::command]
pub fn create_prompt(
    store: State<Arc<Store>>,
    name: String,
    text: String,
) -> Result<PromptList, String> {
    let p = store
        .create_prompt(&name, &text)
        .map_err(|e| e.to_string())?;
    prompt_list(&store, Some(p.id))
}

#[tauri::command]
pub fn duplicate_prompt(store: State<Arc<Store>>, id: String) -> Result<PromptList, String> {
    let p = store.duplicate_prompt(&id).map_err(|e| e.to_string())?;
    prompt_list(&store, Some(p.id))
}

#[tauri::command]
pub fn update_prompt(
    store: State<Arc<Store>>,
    id: String,
    name: String,
    text: String,
) -> Result<PromptList, String> {
    store
        .update_prompt(&id, &name, &text)
        .map_err(|e| e.to_string())?;
    prompt_list(&store, None)
}

#[tauri::command]
pub fn delete_prompt(store: State<Arc<Store>>, id: String) -> Result<PromptList, String> {
    store.delete_prompt(&id).map_err(|e| e.to_string())?;
    prompt_list(&store, None)
}

#[tauri::command]
pub fn set_prompt_starred(
    store: State<Arc<Store>>,
    id: String,
    starred: bool,
) -> Result<PromptList, String> {
    store
        .set_prompt_starred(&id, starred)
        .map_err(|e| e.to_string())?;
    prompt_list(&store, None)
}

#[tauri::command]
pub fn set_active_prompt(store: State<Arc<Store>>, id: String) -> Result<PromptList, String> {
    store.set_active_prompt(&id).map_err(|e| e.to_string())?;
    prompt_list(&store, None)
}

/// One summary model for the picker (parity spec 03 R18, R23).
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SummaryModel {
    pub spec: ModelSpec,
    pub selected: bool,
    pub state: ModelState,
}

#[tauri::command]
pub fn list_summary_models(models: State<Arc<ModelManager>>) -> Vec<SummaryModel> {
    summary_models(&models)
}

pub fn summary_models(models: &ModelManager) -> Vec<SummaryModel> {
    let selected = models.selected_llm();
    models
        .specs()
        .iter()
        .filter(|s| s.kind == ModelKind::Llm)
        .map(|s| SummaryModel {
            spec: s.clone(),
            selected: s.id == selected,
            state: models.readiness.get(&s.id),
        })
        .collect()
}

/// R20: refused while a summary runs or a recording is active.
pub const MODEL_SWITCH_BUSY: &str =
    "Wait for the current summary or recording to finish before changing the model.";

/// Saves and applies a new summary model. Returns the previous one, to
/// revert to if the new one fails to download.
pub fn switch_summary_model(
    store: &Store,
    models: &ModelManager,
    busy: bool,
    model_id: &str,
) -> Result<String, String> {
    if busy {
        return Err(MODEL_SWITCH_BUSY.into());
    }
    let previous = models.selected_llm();
    models.select_llm(model_id)?;
    if let Err(e) = store.set_setting(SUMMARY_MODEL_ID, model_id) {
        let _ = models.select_llm(&previous);
        return Err(e.to_string());
    }
    Ok(previous)
}

/// R19: after `failed` could not download, goes back to `previous`, unless
/// the user already picked something else or another download of it is
/// still running.
pub fn revert_summary_model(
    store: &Store,
    models: &ModelManager,
    failed: &str,
    previous: &str,
) -> bool {
    if models.selected_llm() != failed
        || !matches!(
            models.readiness.get(failed),
            ModelState::Failed | ModelState::Missing
        )
        || models.is_downloading(failed)
        || models.readiness.get(previous) != ModelState::Ready
        || models.select_llm(previous).is_err()
    {
        return false;
    }
    if let Err(e) = store.set_setting(SUMMARY_MODEL_ID, previous) {
        let _ = models.select_llm(failed);
        eprintln!("reverting the summary model failed: {e}");
        return false;
    }
    true
}

/// Switches the summary model without a relaunch; a model not yet on disk
/// downloads in the background, with progress on [`MODELS_PROGRESS_EVENT`].
#[tauri::command]
pub fn set_summary_model(
    app: AppHandle,
    store: State<Arc<Store>>,
    models: State<Arc<ModelManager>>,
    jobs: State<Arc<JobQueue>>,
    recorder: State<Arc<Recorder>>,
    model_id: String,
) -> Result<(), String> {
    let switched = recorder
        .while_idle(|| jobs.while_idle(|| switch_summary_model(&store, &models, false, &model_id)))
        .map_err(|_| MODEL_SWITCH_BUSY.to_owned())?
        .ok_or_else(|| MODEL_SWITCH_BUSY.to_owned())?;
    let previous = switched?;
    let _ = app.emit(SUMMARY_MODEL_CHANGED_EVENT, &model_id);
    if models.readiness.get(&model_id) == ModelState::Ready {
        return Ok(());
    }
    let (store, models) = (Arc::clone(&store), Arc::clone(&models));
    tauri::async_runtime::spawn(async move {
        let result = models
            .ensure_one(&model_id, &|p| {
                let _ = app.emit(MODELS_PROGRESS_EVENT, p);
            })
            .await;
        if let Err(e) = result {
            eprintln!("summary model {model_id}: {e}");
            if revert_summary_model(&store, &models, &model_id, &previous) {
                let _ = app.emit(SUMMARY_MODEL_CHANGED_EVENT, &previous);
            }
        }
    });
    Ok(())
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

#[tauri::command]
pub fn download_model(
    app: AppHandle,
    models: State<Arc<ModelManager>>,
    model_id: String,
) -> Result<(), String> {
    if models.path(&model_id).is_none() {
        return Err(format!("unknown model {model_id}"));
    }
    let models = Arc::clone(&models);
    tauri::async_runtime::spawn(async move {
        models
            .ensure_one(&model_id, &|p| {
                let _ = app.emit(MODELS_PROGRESS_EVENT, p);
            })
            .await
    });
    Ok(())
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
    models: State<Arc<ModelManager>>,
    settings: RecordingSettings,
) -> Result<(), String> {
    if settings.multi_speaker && models.readiness.get(SPEAKER_CAMPPLUS) != ModelState::Ready {
        return Err("download the speaker model before enabling multi-speaker".into());
    }
    store
        .set_recording_settings(&settings)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn confirm_speaker_labels(
    store: State<Arc<Store>>,
    jobs: State<Arc<JobQueue>>,
    id: String,
    speaker_map: std::collections::BTreeMap<String, String>,
) -> Result<String, String> {
    confirm_speaker_labels_with_jobs(store.inner(), jobs.inner(), &id, speaker_map)
}

fn confirm_speaker_labels_with_jobs(
    store: &Store,
    jobs: &JobQueue,
    id: &str,
    speaker_map: std::collections::BTreeMap<String, String>,
) -> Result<String, String> {
    let current = store
        .get_session(id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("session {id} not found"))?;
    let regenerate = current.speaker_map.as_ref() != Some(&speaker_map);
    let original_transcript = current.transcript.clone();
    let original_map = current.speaker_map.clone().unwrap_or_default();
    let original_state = current.diarization_state;
    let prompt = if regenerate {
        Some(
            current
                .prompt_id
                .clone()
                .or(store
                    .get_setting(crate::store::ACTIVE_PROMPT_ID)
                    .map_err(|e| e.to_string())?)
                .ok_or("no summary prompt is selected")?,
        )
    } else {
        None
    };
    let transcript = store
        .confirm_speaker_labels(id, &speaker_map)
        .map_err(|e| e.to_string())?;
    if let Some(prompt) = prompt {
        if let Err(error) = jobs.regenerate(id, &prompt) {
            store
                .update_diarization(
                    id,
                    &original_transcript,
                    &original_map,
                    original_state,
                    None,
                )
                .map_err(|e| e.to_string())?;
            return Err(error.to_string());
        }
    }
    Ok(transcript)
}

#[tauri::command]
pub fn skip_diarization(store: State<Arc<Store>>, id: String) -> Result<String, String> {
    store.skip_diarization(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_speaker_map(
    store: State<Arc<Store>>,
    id: String,
    speaker_map: std::collections::BTreeMap<String, String>,
) -> Result<String, String> {
    store
        .update_speaker_map(&id, &speaker_map)
        .map_err(|e| e.to_string())
}

/// Why kept audio can't be played (parity spec 01 R20/R22/R24). The webview
/// picks its message from the variant; details stay in the log.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum PlaybackError {
    NotFound,
    Expired,
    TooLarge,
    Failed,
}

/// A session's kept audio as WAV bytes (raw IPC body, not a JSON array),
/// built in memory. Off the main thread: a long session is hundreds of MB.
#[tauri::command]
pub async fn request_session_audio(
    store: State<'_, Arc<Store>>,
    id: String,
) -> Result<tauri::ipc::Response, PlaybackError> {
    let store = Arc::clone(&store);
    let audio = tauri::async_runtime::spawn_blocking(move || store.kept_audio(&id, now_ms()))
        .await
        .map_err(|e| {
            eprintln!("request_session_audio: {e}");
            PlaybackError::Failed
        })?
        .map_err(|e| {
            eprintln!("request_session_audio: {e}");
            PlaybackError::Failed
        })?;
    match audio {
        KeptAudio::NotFound => Err(PlaybackError::NotFound),
        KeptAudio::Expired => Err(PlaybackError::Expired),
        KeptAudio::TooLarge => Err(PlaybackError::TooLarge),
        KeptAudio::Streams { mic, system } => Ok(tauri::ipc::Response::new(
            crate::audio::wav::mixed_wav(&mic, &system),
        )),
    }
}

/// Managed always. When `reason` is set, no `Store` is managed and the webview
/// shows only the locked screen (spec 01 R7).
pub struct LockedState {
    pub reason: Option<LockReason>,
    pub db_path: PathBuf,
    pub keys: Arc<dyn KeyStore>,
}

/// What the webview asks first: `null` when the store opened.
#[tauri::command]
pub fn locked_state(locked: State<'_, LockedState>) -> Option<LockReason> {
    locked.reason
}

/// The phrase the user types to confirm "Start fresh".
pub const START_FRESH_CONFIRMATION: &str = "start fresh";

/// R7: set the unreadable DB aside (renamed, not deleted), create a new one
/// with a new key, then restart so setup runs unlocked.
#[tauri::command]
pub fn start_fresh(
    app: AppHandle,
    locked: State<'_, LockedState>,
    confirmation: String,
) -> Result<(), String> {
    if !matches!(
        locked.reason,
        Some(LockReason::KeyMissing | LockReason::WrongKey)
    ) {
        return Err("Start fresh is only offered when the key is missing or wrong".into());
    }
    if !confirmation
        .trim()
        .eq_ignore_ascii_case(START_FRESH_CONFIRMATION)
    {
        return Err(format!("Type \"{START_FRESH_CONFIRMATION}\" to confirm"));
    }
    drop(Store::start_fresh(&locked.db_path, locked.keys.as_ref()).map_err(|e| e.to_string())?);
    app.restart()
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
}

/// True once, after a prototype DB was encrypted (R9 notice).
#[tauri::command]
pub fn take_encryption_notice(store: State<'_, Arc<Store>>) -> Result<bool, String> {
    store.take_encryption_notice().map_err(|e| e.to_string())
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
    licensing: State<'_, Arc<Licensing>>,
) -> Result<Session, String> {
    licensing.require_access()?;
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
pub async fn stop_recording(
    recorder: State<'_, Arc<Recorder>>,
    num_speakers: Option<u8>,
) -> Result<String, String> {
    if !matches!(num_speakers, None | Some(2 | 3)) {
        return Err("speaker count must be 2, 3, or 4+".into());
    }
    let recorder = Arc::clone(&recorder);
    tauri::async_runtime::spawn_blocking(move || {
        recorder
            .stop_with_speakers(num_speakers)
            .map_err(|e| e.to_string())
    })
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
    licensing: State<'_, Arc<Licensing>>,
) -> Result<Vec<TranscriptSegment>, String> {
    licensing.require_access()?;
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

/// The current access state (spec 04 R8).
#[tauri::command]
pub fn get_access_state(licensing: State<Arc<Licensing>>) -> AccessState {
    licensing.state()
}

/// The stored key as `abcd***-123`, for Settings.
#[tauri::command]
pub fn get_masked_license_key(licensing: State<Arc<Licensing>>) -> Option<String> {
    licensing.masked_key()
}

/// Validates and stores a key (R5).
#[tauri::command]
pub async fn activate_license(
    app: AppHandle,
    licensing: State<'_, Arc<Licensing>>,
    key: String,
) -> Result<ActivationResult, String> {
    let licensing = Arc::clone(&licensing);
    let result = tauri::async_runtime::spawn_blocking({
        let licensing = Arc::clone(&licensing);
        move || licensing.activate(&key)
    })
    .await
    .map_err(|e| e.to_string())?;
    emit_access(&app, &licensing);
    Ok(result)
}

/// Forgets the key locally; no network call (R13).
#[tauri::command]
pub fn deactivate_license(app: AppHandle, licensing: State<Arc<Licensing>>) {
    licensing.deactivate();
    emit_access(&app, &licensing);
}

pub fn emit_access(app: &AppHandle, licensing: &Licensing) {
    let _ = app.emit(LICENSE_CHANGED_EVENT, licensing.state());
}

/// Opens an http(s) or mailto link in the default handler (spec 04 R11, the
/// shape spec 08 R22 defines).
#[tauri::command]
pub fn open_external_link(url: String) -> Result<(), String> {
    open_link_with(&url, |u| {
        std::process::Command::new("/usr/bin/open")
            .arg(u)
            .status()
            .map_err(|e| e.to_string())
            .and_then(|s| {
                s.success()
                    .then_some(())
                    .ok_or_else(|| "the link did not open".to_string())
            })
    })
}

fn open_link_with(
    url: &str,
    launch: impl FnOnce(&str) -> Result<(), String>,
) -> Result<(), String> {
    let parsed = reqwest::Url::parse(url).map_err(|e| format!("not a link: {e}"))?;
    match parsed.scheme() {
        "http" | "https" | "mailto" => launch(parsed.as_str()),
        other => Err(format!("links of type {other} are not opened")),
    }
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

    /// AS10 (structure): only the two commands that create a session from
    /// new audio call the gate; reading, editing and settings never do.
    #[test]
    fn as10_only_new_audio_commands_are_gated() {
        let src = include_str!("commands.rs");
        let src = &src[..src.find("#[cfg(test)]\nmod tests").unwrap()];
        let mut gated = Vec::new();
        let mut commands = Vec::new();
        for chunk in src.split("#[tauri::command]").skip(1) {
            let name = chunk
                .split("fn ")
                .nth(1)
                .and_then(|r| r.split(['(', '<']).next())
                .unwrap()
                .to_string();
            if chunk.contains("require_access(") {
                gated.push(name.clone());
            }
            commands.push(name);
        }
        assert_eq!(gated, ["start_recording", "transcribe_file"]);
        for open in [
            "list_sessions",
            "get_session",
            "update_session",
            "delete_session",
            "regenerate_summary",
            "set_recording_settings",
            "stop_recording",
            "activate_license",
        ] {
            assert!(commands.iter().any(|c| c == open), "{open} is a command");
        }
    }

    #[test]
    fn external_links_are_allow_listed() {
        let mut opened = Vec::new();
        for ok in [
            "https://www.scribular.com/#pricing",
            "http://example.com",
            "mailto:support@scribular.com",
        ] {
            open_link_with(ok, |u| {
                opened.push(u.to_string());
                Ok(())
            })
            .unwrap();
        }
        assert_eq!(opened[0], "https://www.scribular.com/#pricing");
        for bad in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "x-apple.systempreferences:",
            "not a url",
        ] {
            assert!(
                open_link_with(bad, |_| panic!("launched {bad}")).is_err(),
                "{bad}"
            );
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

    #[cfg(feature = "fake-engines")]
    #[test]
    fn label_confirmation_retries_the_same_names_after_a_busy_summary() {
        use crate::{
            engines::fake::{FakeLoader, FakeSummarizer},
            jobs::{JobEvents, SessionProgress, SessionSegments, SummaryDelta},
            store::{key::MemoryKeyStore, DiarizationState, NewSession, SessionStatus},
        };

        struct Events;
        impl JobEvents for Events {
            fn progress(&self, _: SessionProgress) {}
            fn summary_delta(&self, _: SummaryDelta) {}
            fn segments(&self, _: SessionSegments) {}
        }

        let path =
            std::env::temp_dir().join(format!("scribular-confirm-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let store = Arc::new(Store::open(&path, &MemoryKeyStore::with_key([8; 32])).unwrap());
        let mut summarizer = FakeSummarizer::new(8192);
        summarizer.token_delay = std::time::Duration::from_millis(2);
        let loader = Arc::new(FakeLoader::new(summarizer));
        let jobs = JobQueue::new(store.clone(), loader.clone(), Arc::new(Events));
        let session = store
            .create_session(&NewSession {
                title: "retry".into(),
                input_mode: InputMode::Mic,
                prompt_id: Some("default-therapy-dora".into()),
                model_id: None,
            })
            .unwrap();
        let plain = (0..1000)
            .map(|i| format!("word{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        let defaults =
            std::collections::BTreeMap::from([("SPEAKER_0".to_string(), "Speaker 1".to_string())]);
        let raw = format!(
            "SPEAKER_0: {}",
            (0..80)
                .map(|i| format!("hello{i}"))
                .collect::<Vec<_>>()
                .join(" ")
        );
        store
            .save_diarization(&session.id, &plain, &raw, &defaults, 1000)
            .unwrap();
        store
            .set_status(&session.id, SessionStatus::Done, None)
            .unwrap();
        jobs.regenerate(&session.id, "default-therapy-dora")
            .unwrap();

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while store.get_session(&session.id).unwrap().unwrap().status != SessionStatus::Summarizing
        {
            assert!(
                std::time::Instant::now() < deadline,
                "initial summary never started"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let names =
            std::collections::BTreeMap::from([("SPEAKER_0".to_string(), "Therapist".to_string())]);
        assert!(
            confirm_speaker_labels_with_jobs(&store, &jobs, &session.id, names.clone()).is_err()
        );
        let restored = store.get_session(&session.id).unwrap().unwrap();
        assert_eq!(restored.diarization_state, DiarizationState::Pending);
        assert_eq!(restored.speaker_map.as_ref(), Some(&defaults));
        assert_eq!(restored.transcript, plain);

        assert!(jobs.wait_idle(std::time::Duration::from_secs(10)));
        assert_eq!(
            confirm_speaker_labels_with_jobs(&store, &jobs, &session.id, names).unwrap(),
            format!(
                "Therapist: {}",
                (0..80)
                    .map(|i| format!("hello{i}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            )
        );
        assert!(jobs.wait_idle(std::time::Duration::from_secs(10)));
        assert!(loader
            .summarizer
            .calls()
            .iter()
            .flatten()
            .any(|generation| generation.prompt.contains("Therapist: hello0")));
        drop(jobs);
        drop(store);
        let _ = std::fs::remove_file(path);
    }
}
