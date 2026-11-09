//! Model manager (KTD8): downloads the pinned models into
//! `appDataDir/models`, reports progress, and publishes each model's state
//! on a readiness channel that the job queue waits on.

mod download;
mod manifest;

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex, OnceLock,
    },
    time::Duration,
};

use serde::Serialize;
use tauri::async_runtime::Mutex as AsyncMutex;
use ts_rs::TS;

pub use download::{ensure, model_path, verify_existing, DownloadError};
pub use manifest::{
    manifest, ModelKind, ModelSpec, DEFAULT_LLM, GEMMA_4_E2B, SPEAKER_CAMPPLUS, WHISPER_BASE,
};

// shortcut: provisional 8 GiB floor, replace if AS10 measures a higher minimum.
pub const MIN_SUMMARY_RAM_BYTES: u64 = 8 << 30;
pub const SUMMARY_AVAILABILITY_EVENT: &str = "summary-availability";
const OLD_SUMMARY_MODEL_FILE: &str = "gemma-2-2b-it-Q4_K_M.gguf";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum SummaryAvailability {
    Ready,
    Downloading,
    Unsupported,
    LoadFailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum ModelState {
    Missing,
    Downloading,
    Verifying,
    Ready,
    Failed,
}

/// Payload of `models://progress`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ModelProgress {
    pub model_id: String,
    pub state: ModelState,
    #[ts(type = "number")]
    pub downloaded_bytes: u64,
    #[ts(type = "number")]
    pub total_bytes: u64,
    pub error: Option<String>,
}

/// Latest state per model id, with blocking waits for job threads.
#[derive(Default)]
pub struct Readiness {
    states: Mutex<HashMap<String, ModelState>>,
    changed: Condvar,
}

impl Readiness {
    pub fn get(&self, model_id: &str) -> ModelState {
        let states = self.states.lock().unwrap_or_else(|p| p.into_inner());
        states.get(model_id).copied().unwrap_or(ModelState::Missing)
    }

    pub fn set(&self, model_id: &str, state: ModelState) {
        let mut states = self.states.lock().unwrap_or_else(|p| p.into_inner());
        states.insert(model_id.to_string(), state);
        self.changed.notify_all();
    }

    /// Blocks until `model_id` is `Ready` or `Failed`, or `timeout` passes;
    /// returns the state at that point.
    pub fn wait_settled(&self, model_id: &str, timeout: Duration) -> ModelState {
        let states = self.states.lock().unwrap_or_else(|p| p.into_inner());
        let (states, _) = self
            .changed
            .wait_timeout_while(states, timeout, |s| {
                !matches!(
                    s.get(model_id),
                    Some(ModelState::Ready | ModelState::Failed)
                )
            })
            .unwrap_or_else(|p| p.into_inner());
        states.get(model_id).copied().unwrap_or(ModelState::Missing)
    }
}

pub struct ModelManager {
    dir: PathBuf,
    specs: Vec<ModelSpec>,
    client: reqwest::Client,
    /// One guard per model: retries can proceed while a later model downloads.
    running: Vec<Arc<AtomicBool>>,
    /// Serializes requests so concurrent callers see an owner's result before retrying.
    download_locks: Vec<AsyncMutex<()>>,
    pub readiness: Readiness,
    /// The summary model (spec 03 R18); only it downloads with the rest.
    selected_llm: Mutex<String>,
    /// The recorder's update install gate, shared so downloads cannot start mid-install.
    install_gate: OnceLock<Arc<Mutex<()>>>,
    availability: Mutex<SummaryAvailability>,
    availability_event: OnceLock<tauri::AppHandle>,
}

struct DownloadGuard(Arc<AtomicBool>);

impl DownloadGuard {
    /// Claims `flag`; None when a download already holds it.
    fn claim(flag: Arc<AtomicBool>) -> Option<Self> {
        (!flag.swap(true, Ordering::AcqRel)).then(|| Self(flag))
    }
}

impl Drop for DownloadGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

impl ModelManager {
    pub fn new(dir: PathBuf, specs: Vec<ModelSpec>) -> Self {
        Self::new_with_memory(dir, specs, crate::hardware::physical_memory_bytes())
    }

    fn new_with_memory(dir: PathBuf, specs: Vec<ModelSpec>, memory_bytes: Option<u64>) -> Self {
        let first_llm = specs
            .iter()
            .find(|s| s.id == DEFAULT_LLM && s.kind == ModelKind::Llm)
            .or_else(|| specs.iter().find(|s| s.kind == ModelKind::Llm))
            .map(|s| s.id.clone())
            .unwrap_or_default();
        let unsupported = memory_bytes.is_some_and(|bytes| bytes < MIN_SUMMARY_RAM_BYTES);
        Self {
            dir,
            running: specs
                .iter()
                .map(|_| Arc::new(AtomicBool::new(false)))
                .collect(),
            download_locks: specs.iter().map(|_| AsyncMutex::new(())).collect(),
            specs,
            client: reqwest::Client::new(),
            readiness: Readiness::default(),
            selected_llm: Mutex::new(first_llm),
            install_gate: OnceLock::new(),
            availability: Mutex::new(if unsupported {
                SummaryAvailability::Unsupported
            } else {
                SummaryAvailability::Downloading
            }),
            availability_event: OnceLock::new(),
        }
    }

    pub fn set_availability_event(&self, app: tauri::AppHandle) {
        let _ = self.availability_event.set(app);
    }

    pub fn summary_availability(&self) -> SummaryAvailability {
        *self.availability.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn has_legacy_summary_model(&self) -> bool {
        self.dir.join(OLD_SUMMARY_MODEL_FILE).is_file()
    }

    pub fn remove_legacy_summary_model(&self) {
        remove_old_summary_model(&self.dir);
    }

    pub fn set_summary_availability(&self, state: SummaryAvailability) {
        let mut current = self.availability.lock().unwrap_or_else(|p| p.into_inner());
        if *current == state {
            return;
        }
        *current = state;
        if let Some(app) = self.availability_event.get() {
            use tauri::Emitter;
            let _ = app.emit(SUMMARY_AVAILABILITY_EVENT, state);
        }
    }

    pub fn set_install_gate(&self, gate: Arc<Mutex<()>>) {
        let _ = self.install_gate.set(gate);
    }

    pub fn selected_llm(&self) -> String {
        self.selected_llm.lock().unwrap().clone()
    }

    pub fn is_downloading(&self, model_id: &str) -> bool {
        self.specs
            .iter()
            .position(|spec| spec.id == model_id)
            .is_some_and(|i| self.running[i].load(Ordering::Acquire))
    }

    pub fn is_busy(&self) -> bool {
        self.running
            .iter()
            .any(|running| running.load(Ordering::Acquire))
    }

    /// Makes `model_id` the summary model. Fails for an id that is not an LLM.
    pub fn select_llm(&self, model_id: &str) -> Result<(), String> {
        if !self
            .specs
            .iter()
            .any(|s| s.id == model_id && s.kind == ModelKind::Llm)
        {
            return Err(format!("unknown summary model {model_id}"));
        }
        *self.selected_llm.lock().unwrap() = model_id.into();
        Ok(())
    }

    pub fn spec(&self, model_id: &str) -> Option<&ModelSpec> {
        self.specs.iter().find(|s| s.id == model_id)
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn specs(&self) -> &[ModelSpec] {
        &self.specs
    }

    pub fn path(&self, model_id: &str) -> Option<PathBuf> {
        let spec = self.specs.iter().find(|s| s.id == model_id)?;
        Some(model_path(&self.dir, spec))
    }

    /// Marks models already on disk and verified as Ready. Never touches
    /// the network; a model without its `.sha256` stamp is re-hashed.
    pub fn scan(&self) {
        for spec in &self.specs {
            if verify_existing(&self.dir, spec).unwrap_or(false) {
                self.readiness.set(&spec.id, ModelState::Ready);
            }
        }
        if self.summary_availability() != SummaryAvailability::Unsupported
            && self.readiness.get(&self.selected_llm()) == ModelState::Ready
        {
            self.set_summary_availability(SummaryAvailability::Ready);
        }
    }

    /// Current state of every model, for the first render before any
    /// progress event arrives.
    pub fn status(&self) -> Vec<ModelProgress> {
        self.specs
            .iter()
            .map(|spec| {
                let state = self.readiness.get(&spec.id);
                ModelProgress {
                    model_id: spec.id.clone(),
                    state,
                    downloaded_bytes: if state == ModelState::Ready {
                        spec.size
                    } else {
                        0
                    },
                    total_bytes: spec.size,
                    error: None,
                }
            })
            .collect()
    }

    /// Ensures models in manifest order. A failed speech download can be
    /// retried during the LLM download, with at most one download per model.
    /// Stops at an active model so concurrent calls cannot skip ahead.
    pub async fn ensure_all(&self, on_progress: &(dyn Fn(ModelProgress) + Send + Sync)) {
        self.ensure_all_with_memory(crate::hardware::physical_memory_bytes(), on_progress)
            .await;
    }

    async fn ensure_all_with_memory(
        &self,
        memory_bytes: Option<u64>,
        on_progress: &(dyn Fn(ModelProgress) + Send + Sync),
    ) {
        let unsupported = memory_bytes.is_some_and(|bytes| bytes < MIN_SUMMARY_RAM_BYTES);
        if unsupported {
            self.set_summary_availability(SummaryAvailability::Unsupported);
            remove_old_summary_model(&self.dir);
        }
        for i in 0..self.specs.len() {
            let spec = &self.specs[i];
            let selected = self.selected_llm();
            if !spec.optional
                && (spec.kind != ModelKind::Llm || (spec.id == selected && !unsupported))
            {
                if self.running[i].load(Ordering::Acquire) {
                    return;
                }
                if self.ensure_index(i, on_progress).await.is_none() {
                    return;
                }
            }
        }
    }

    /// Downloads one model without starting other optional downloads.
    pub async fn ensure_one(
        &self,
        model_id: &str,
        on_progress: &(dyn Fn(ModelProgress) + Send + Sync),
    ) -> Result<(), String> {
        let i = self
            .specs
            .iter()
            .position(|spec| spec.id == model_id)
            .ok_or_else(|| format!("unknown model {model_id}"))?;
        if self.specs[i].kind == ModelKind::Llm
            && self.summary_availability() == SummaryAvailability::Unsupported
        {
            return Err("summary model is unsupported on this Mac".into());
        }
        match self.ensure_index(i, on_progress).await {
            Some(true) => Ok(()),
            Some(false) => Err(format!("model {model_id} download failed")),
            None => Err(format!("model {model_id} download is already running")),
        }
    }

    async fn ensure_index(
        &self,
        i: usize,
        on_progress: &(dyn Fn(ModelProgress) + Send + Sync),
    ) -> Option<bool> {
        let _download = self.download_locks[i].lock().await;
        let spec = &self.specs[i];
        let running = Arc::clone(&self.running[i]);
        // Claimed under the install gate: an update install either sees this
        // download as busy or restarts before it starts.
        // The gate is a std Mutex an install can hold for a while; wait for it
        // off the async workers. The guard is created inside the blocking task,
        // so a cancelled waiter still drops it and clears the claim.
        let _guard = match self.install_gate.get().cloned() {
            Some(gate) => tauri::async_runtime::spawn_blocking(move || {
                let _gate = gate.lock().unwrap_or_else(|p| p.into_inner());
                DownloadGuard::claim(running)
            })
            .await
            // The closure cannot panic; treat a join failure as busy.
            .unwrap_or(None),
            None => DownloadGuard::claim(running),
        };
        if _guard.is_none() {
            return None;
        }
        if self.readiness.get(&spec.id) == ModelState::Ready {
            on_progress(ModelProgress {
                model_id: spec.id.clone(),
                state: ModelState::Ready,
                downloaded_bytes: spec.size,
                total_bytes: spec.size,
                error: None,
            });
            if spec.kind == ModelKind::Llm {
                self.set_summary_availability(SummaryAvailability::Ready);
            }
            return Some(true);
        }
        if spec.kind == ModelKind::Llm {
            self.set_summary_availability(SummaryAvailability::Downloading);
        }
        let report = |p: ModelProgress| {
            self.readiness.set(&p.model_id, p.state);
            on_progress(p);
        };
        match ensure(&self.client, &self.dir, spec, &report).await {
            Ok(_) => {
                if spec.kind == ModelKind::Llm {
                    self.set_summary_availability(SummaryAvailability::Ready);
                }
                Some(true)
            }
            Err(e) => {
                report(ModelProgress {
                    model_id: spec.id.clone(),
                    state: ModelState::Failed,
                    downloaded_bytes: 0,
                    total_bytes: spec.size,
                    error: Some(e.to_string()),
                });
                if spec.kind == ModelKind::Llm {
                    self.set_summary_availability(SummaryAvailability::LoadFailed);
                }
                Some(false)
            }
        }
    }
}

fn remove_old_summary_model(dir: &Path) {
    if let Err(e) = fs::remove_file(dir.join(OLD_SUMMARY_MODEL_FILE)) {
        if e.kind() != std::io::ErrorKind::NotFound {
            crate::diag::log::log("WARN", "models", "old summary model cleanup failed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructing_on_low_memory_does_not_delete_old_summary() {
        let dir = std::env::temp_dir().join(format!("scribular-model-gate-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let old = dir.join(OLD_SUMMARY_MODEL_FILE);
        fs::write(&old, b"old model").unwrap();

        let manager =
            ModelManager::new_with_memory(dir.clone(), manifest(), Some(MIN_SUMMARY_RAM_BYTES - 1));

        assert_eq!(
            manager.summary_availability(),
            SummaryAvailability::Unsupported
        );
        assert!(old.exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn unknown_memory_does_not_fail_the_summary_gate() {
        let dir =
            std::env::temp_dir().join(format!("scribular-model-unknown-{}", std::process::id()));
        let manager = ModelManager::new_with_memory(dir, manifest(), None);
        assert_eq!(
            manager.summary_availability(),
            SummaryAvailability::Downloading
        );
    }

    #[test]
    fn low_memory_keeps_speech_available_and_skips_the_llm_download() {
        let dir = std::env::temp_dir().join(format!("scribular-model-skip-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let old = dir.join(OLD_SUMMARY_MODEL_FILE);
        fs::write(&old, b"old model").unwrap();
        let manager = ModelManager::new_with_memory(
            dir.clone(),
            manifest(),
            Some(MIN_SUMMARY_RAM_BYTES - 1),
        );
        manager.readiness.set(WHISPER_BASE, ModelState::Ready);
        let seen = Mutex::new(Vec::new());

        tauri::async_runtime::block_on(manager.ensure_all_with_memory(
            Some(MIN_SUMMARY_RAM_BYTES - 1),
            &|progress| seen.lock().unwrap().push(progress.model_id),
        ));

        assert_eq!(*seen.lock().unwrap(), [WHISPER_BASE]);
        assert_eq!(manager.readiness.get(DEFAULT_LLM), ModelState::Missing);
        assert_eq!(manager.summary_availability(), SummaryAvailability::Unsupported);
        assert!(!old.exists());
        fs::remove_dir_all(dir).unwrap();
    }
}
