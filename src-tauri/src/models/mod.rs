//! Model manager (KTD8): downloads the pinned models into
//! `appDataDir/models`, reports progress, and publishes each model's state
//! on a readiness channel that the job queue waits on.

mod download;
mod manifest;

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Condvar, Mutex,
    },
    time::Duration,
};

use serde::Serialize;
use ts_rs::TS;

pub use download::{ensure, model_path, verify_existing, DownloadError};
pub use manifest::{manifest, ModelKind, ModelSpec, GEMMA_2_2B, SPEAKER_CAMPPLUS, WHISPER_BASE};

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
    running: Vec<AtomicBool>,
    pub readiness: Readiness,
}

struct DownloadGuard<'a>(&'a AtomicBool);

impl Drop for DownloadGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

impl ModelManager {
    pub fn new(dir: PathBuf, specs: Vec<ModelSpec>) -> Self {
        Self {
            dir,
            running: specs.iter().map(|_| AtomicBool::new(false)).collect(),
            specs,
            client: reqwest::Client::new(),
            readiness: Readiness::default(),
        }
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
        for i in 0..self.specs.len() {
            if !self.specs[i].optional {
                if self.running[i].load(Ordering::Acquire) {
                    return;
                }
                self.ensure_index(i, on_progress).await;
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
        if self.ensure_index(i, on_progress).await {
            Ok(())
        } else {
            Err(format!("model {model_id} download failed"))
        }
    }

    async fn ensure_index(
        &self,
        i: usize,
        on_progress: &(dyn Fn(ModelProgress) + Send + Sync),
    ) -> bool {
        let spec = &self.specs[i];
        let running = &self.running[i];
        if running.swap(true, Ordering::AcqRel) {
            return false;
        }
        let _guard = DownloadGuard(running);
        if self.readiness.get(&spec.id) == ModelState::Ready {
            on_progress(ModelProgress {
                model_id: spec.id.clone(),
                state: ModelState::Ready,
                downloaded_bytes: spec.size,
                total_bytes: spec.size,
                error: None,
            });
            return true;
        }
        let report = |p: ModelProgress| {
            self.readiness.set(&p.model_id, p.state);
            on_progress(p);
        };
        match ensure(&self.client, &self.dir, spec, &report).await {
            Ok(_) => true,
            Err(e) => {
                report(ModelProgress {
                    model_id: spec.id.clone(),
                    state: ModelState::Failed,
                    downloaded_bytes: 0,
                    total_bytes: spec.size,
                    error: Some(e.to_string()),
                });
                false
            }
        }
    }
}
