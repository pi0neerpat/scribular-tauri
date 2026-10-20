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
pub use manifest::{manifest, ModelKind, ModelSpec, GEMMA_2_2B, WHISPER_BASE};

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
    /// Set while `ensure_all` runs, so a second call doesn't race it.
    running: AtomicBool,
    pub readiness: Readiness,
}

impl ModelManager {
    pub fn new(dir: PathBuf, specs: Vec<ModelSpec>) -> Self {
        Self {
            dir,
            specs,
            client: reqwest::Client::new(),
            running: AtomicBool::new(false),
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

    /// Ensures every model in manifest order, one at a time. A failure is
    /// reported and the next model still downloads. Returns at once if
    /// another call is already running.
    pub async fn ensure_all(&self, on_progress: &(dyn Fn(ModelProgress) + Send + Sync)) {
        if self.running.swap(true, Ordering::AcqRel) {
            return;
        }
        struct Done<'a>(&'a AtomicBool);
        impl Drop for Done<'_> {
            fn drop(&mut self) {
                self.0.store(false, Ordering::Release);
            }
        }
        let _done = Done(&self.running);
        for spec in &self.specs {
            if self.readiness.get(&spec.id) == ModelState::Ready {
                continue;
            }
            let report = |p: ModelProgress| {
                self.readiness.set(&p.model_id, p.state);
                on_progress(p);
            };
            if let Err(e) = ensure(&self.client, &self.dir, spec, &report).await {
                report(ModelProgress {
                    model_id: spec.id.clone(),
                    state: ModelState::Failed,
                    downloaded_bytes: 0,
                    total_bytes: spec.size,
                    error: Some(e.to_string()),
                });
            }
        }
    }
}
