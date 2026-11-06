use std::sync::{Arc, Mutex, TryLockError};

use serde::Serialize;
use tauri_plugin_updater::{Update, UpdaterExt};
use ts_rs::TS;

use crate::store::{Store, PENDING_UPDATE_INSTALL, SKIPPED_UPDATE_VERSION};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum UpdateState {
    Idle {
        installable: bool,
    },
    Checking {
        installable: bool,
    },
    Downloading {
        installable: bool,
        #[ts(type = "number")]
        received: u64,
        #[ts(type = "number | null")]
        total: Option<u64>,
    },
    Ready {
        version: String,
        notes: String,
        installable: bool,
    },
    Error {
        installable: bool,
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateInfo {
    pub version: String,
    pub notes: String,
}

#[derive(Debug)]
pub enum UpdateSourceError {
    Signature(String),
    Other(String),
    BundleNotRestored(String),
}

impl UpdateSourceError {
    fn message(&self) -> String {
        match self {
            Self::Signature(message) | Self::Other(message) | Self::BundleNotRestored(message) => {
                message.clone()
            }
        }
    }
}

impl From<tauri_plugin_updater::Error> for UpdateSourceError {
    fn from(error: tauri_plugin_updater::Error) -> Self {
        use tauri_plugin_updater::Error::*;
        if matches!(
            &error,
            tauri_plugin_updater::Error::PreviousAppNotRestored(_)
        ) {
            Self::BundleNotRestored(error.to_string())
        } else if matches!(
            &error,
            Minisign(_)
                | Base64(_)
                | SignatureUtf8(_)
                | SignedVersionMismatch { .. }
                | MissingSignedVersion
        ) {
            Self::Signature(error.to_string())
        } else {
            Self::Other(error.to_string())
        }
    }
}

pub trait UpdateSource: Send + Sync {
    fn check(&self) -> Result<Option<UpdateInfo>, UpdateSourceError>;
    fn download(
        &self,
        info: &UpdateInfo,
        progress: &dyn Fn(u64, Option<u64>),
    ) -> Result<Vec<u8>, UpdateSourceError>;
    fn install(&self, info: &UpdateInfo, bytes: &[u8]) -> Result<(), UpdateSourceError>;
}

pub struct TauriUpdateSource {
    app: tauri::AppHandle,
    pending: Mutex<Option<Update>>,
}

impl TauriUpdateSource {
    pub fn new(app: tauri::AppHandle) -> Self {
        Self {
            app,
            pending: Mutex::new(None),
        }
    }
}

impl UpdateSource for TauriUpdateSource {
    fn check(&self) -> Result<Option<UpdateInfo>, UpdateSourceError> {
        let updater = if cfg!(debug_assertions) {
            if let Some(endpoint) = std::env::var_os("SCRIBULAR_UPDATE_ENDPOINT") {
                self.app
                    .updater_builder()
                    .endpoints(vec![endpoint.to_string_lossy().parse().map_err(|e| {
                        UpdateSourceError::Other(format!("invalid SCRIBULAR_UPDATE_ENDPOINT: {e}"))
                    })?])
                    .map_err(|e| UpdateSourceError::Other(e.to_string()))?
                    .build()
                    .map_err(|e| UpdateSourceError::Other(e.to_string()))?
            } else {
                self.app.updater().map_err(UpdateSourceError::from)?
            }
        } else {
            self.app.updater().map_err(UpdateSourceError::from)?
        };
        let update =
            tauri::async_runtime::block_on(updater.check()).map_err(UpdateSourceError::from)?;
        let Some(update) = update else {
            return Ok(None);
        };
        let info = UpdateInfo {
            version: update.version.clone(),
            notes: update.body.clone().unwrap_or_default(),
        };
        *self.pending.lock().unwrap_or_else(|p| p.into_inner()) = Some(update);
        Ok(Some(info))
    }

    fn download(
        &self,
        info: &UpdateInfo,
        progress: &dyn Fn(u64, Option<u64>),
    ) -> Result<Vec<u8>, UpdateSourceError> {
        let update = self
            .pending
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .as_ref()
            .filter(|u| u.version == info.version)
            .cloned()
            .ok_or_else(|| UpdateSourceError::Other("update check expired".into()))?;
        tauri::async_runtime::block_on(
            update.download(|received, total| progress(received as u64, total), || {}),
        )
        .map_err(UpdateSourceError::from)
    }

    fn install(&self, info: &UpdateInfo, bytes: &[u8]) -> Result<(), UpdateSourceError> {
        let update = self
            .pending
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .as_ref()
            .filter(|u| u.version == info.version)
            .cloned()
            .ok_or_else(|| UpdateSourceError::Other("update check expired".into()))?;
        update.install(bytes).map_err(UpdateSourceError::from)
    }
}

pub struct UpdateManager {
    source: Arc<dyn UpdateSource>,
    state: Mutex<UpdateState>,
    staged: Mutex<Option<(UpdateInfo, Vec<u8>)>>,
    busy: Arc<dyn Fn() -> bool + Send + Sync>,
    gate: Arc<Mutex<()>>,
    emit: Arc<dyn Fn(UpdateState) + Send + Sync>,
    restart: Arc<dyn Fn() + Send + Sync>,
    store: Mutex<Option<Arc<Store>>>,
}

impl UpdateManager {
    pub fn new(
        source: Arc<dyn UpdateSource>,
        busy: Arc<dyn Fn() -> bool + Send + Sync>,
        gate: Arc<Mutex<()>>,
        emit: Arc<dyn Fn(UpdateState) + Send + Sync>,
        restart: Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        Self {
            source,
            state: Mutex::new(UpdateState::Idle { installable: false }),
            staged: Mutex::new(None),
            busy,
            gate,
            emit,
            restart,
            store: Mutex::new(None),
        }
    }

    fn publish(&self, state: UpdateState) {
        *self.state.lock().unwrap_or_else(|p| p.into_inner()) = state.clone();
        (self.emit)(state);
    }

    pub fn state(&self) -> UpdateState {
        self.state.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    pub fn check(&self) {
        if matches!(
            self.state(),
            UpdateState::Checking { .. }
                | UpdateState::Downloading { .. }
                | UpdateState::Ready { .. }
        ) {
            return;
        }
        self.publish(UpdateState::Checking { installable: false });
        match self.source.check() {
            Ok(Some(info)) => {
                let current = semver::Version::parse(env!("CARGO_PKG_VERSION"));
                let offered = semver::Version::parse(&info.version);
                if !matches!((current, offered), (Ok(current), Ok(offered)) if offered > current) {
                    self.publish(UpdateState::Idle { installable: false });
                    return;
                }
                if let Some(store) = self
                    .store
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .as_ref()
                {
                    match store.get_setting(SKIPPED_UPDATE_VERSION) {
                        Ok(Some(skipped)) if skipped == info.version => {
                            self.publish(UpdateState::Idle { installable: false });
                            return;
                        }
                        Ok(Some(_)) => {
                            let _ = store.delete_setting(SKIPPED_UPDATE_VERSION);
                        }
                        Ok(None) => {}
                        Err(error) => eprintln!("reading skipped update version failed: {error}"),
                    }
                }
                let update = UpdateState::Downloading {
                    installable: false,
                    received: 0,
                    total: None,
                };
                self.publish(update);
                match self.source.download(&info, &|received, total| {
                    self.publish(UpdateState::Downloading {
                        installable: false,
                        received,
                        total,
                    })
                }) {
                    Ok(bytes) => {
                        *self.staged.lock().unwrap_or_else(|p| p.into_inner()) =
                            Some((info.clone(), bytes));
                        self.publish(UpdateState::Ready {
                            version: info.version,
                            notes: info.notes,
                            installable: !(self.busy)(),
                        });
                    }
                    Err(error) => {
                        *self.staged.lock().unwrap_or_else(|p| p.into_inner()) = None;
                        self.publish(UpdateState::Error {
                            installable: false,
                            message: error.message(),
                        });
                    }
                }
            }
            Ok(None) => self.publish(UpdateState::Idle { installable: false }),
            Err(error) => self.publish(UpdateState::Error {
                installable: false,
                message: error.message(),
            }),
        }
    }

    pub fn refresh_installable(&self) {
        if let UpdateState::Ready {
            version,
            notes,
            installable,
        } = self.state()
        {
            let next = !(self.busy)();
            if next != installable {
                self.publish(UpdateState::Ready {
                    version,
                    notes,
                    installable: next,
                });
            }
        }
    }

    pub fn install(&self) -> Result<(), String> {
        self.install_staged(true)
    }

    fn install_staged(&self, restart: bool) -> Result<(), String> {
        let _gate = if restart {
            self.gate.lock().unwrap_or_else(|p| p.into_inner())
        } else {
            match self.gate.try_lock() {
                Ok(gate) => gate,
                Err(TryLockError::Poisoned(error)) => error.into_inner(),
                Err(TryLockError::WouldBlock) => return Err("updater is busy".into()),
            }
        };
        if (self.busy)() {
            return Err("busy".into());
        }
        let (info, bytes) = self
            .staged
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
            .ok_or("no update is ready")?;
        if let Some(store) = self
            .store
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .as_ref()
        {
            let attempts = store
                .get_setting(PENDING_UPDATE_INSTALL)
                .ok()
                .flatten()
                .and_then(|raw| serde_json::from_str::<PendingInstall>(&raw).ok())
                .filter(|m| m.from == env!("CARGO_PKG_VERSION") && m.to == info.version)
                .map_or(0, |m| m.attempts);
            let marker = PendingInstall {
                from: env!("CARGO_PKG_VERSION").into(),
                to: info.version.clone(),
                attempts,
            };
            let marker = serde_json::to_string(&marker).map_err(|e| e.to_string())?;
            store
                .set_setting(PENDING_UPDATE_INSTALL, &marker)
                .map_err(|e| e.to_string())?;
        }
        self.source.install(&info, &bytes).map_err(|error| {
            let message = error.message();
            let state = match &error {
                UpdateSourceError::BundleNotRestored(_) => UpdateState::Error {
                    installable: false,
                    message: message.clone(),
                },
                _ => UpdateState::Ready {
                    version: info.version.clone(),
                    notes: info.notes.clone(),
                    installable: !(self.busy)(),
                },
            };
            self.publish(state);
            message
        })?;
        *self.staged.lock().unwrap_or_else(|p| p.into_inner()) = None;
        self.publish(UpdateState::Idle { installable: false });
        if restart {
            (self.restart)();
        }
        Ok(())
    }

    pub fn install_on_quit(&self) {
        if !matches!(self.state(), UpdateState::Ready { .. }) {
            return;
        }
        if let Err(error) = self.install_staged(false) {
            eprintln!("install on quit failed: {error}");
        }
    }

    pub fn gate(&self) -> Arc<Mutex<()>> {
        Arc::clone(&self.gate)
    }

    pub fn set_store(&self, store: Arc<Store>) {
        *self.store.lock().unwrap_or_else(|p| p.into_inner()) = Some(Arc::clone(&store));
        self.reconcile_marker_at(&store, env!("CARGO_PKG_VERSION"));
    }

    fn reconcile_marker_at(&self, store: &Store, version: &str) {
        let raw = match store.get_setting(PENDING_UPDATE_INSTALL) {
            Ok(Some(raw)) => raw,
            Ok(None) => return,
            Err(error) => {
                eprintln!("reading pending update install failed: {error}");
                return;
            }
        };
        let Ok(mut marker) = serde_json::from_str::<PendingInstall>(&raw) else {
            eprintln!("discarding invalid pending update marker");
            let _ = store.delete_setting(PENDING_UPDATE_INSTALL);
            return;
        };
        if marker.to == version {
            let _ = store.delete_setting(PENDING_UPDATE_INSTALL);
            eprintln!("update {} installed", marker.to);
        } else if marker.from == version {
            marker.attempts += 1;
            if marker.attempts >= 3 {
                let _ = store.set_setting(SKIPPED_UPDATE_VERSION, &marker.to);
                let _ = store.delete_setting(PENDING_UPDATE_INSTALL);
                eprintln!(
                    "skipping update {} after {} failed installs",
                    marker.to, marker.attempts
                );
            } else if let Ok(raw) = serde_json::to_string(&marker) {
                let _ = store.set_setting(PENDING_UPDATE_INSTALL, &raw);
                eprintln!(
                    "update {} install attempt {} failed",
                    marker.to, marker.attempts
                );
            }
        } else {
            let _ = store.delete_setting(PENDING_UPDATE_INSTALL);
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct PendingInstall {
    from: String,
    to: String,
    attempts: u8,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::key::MemoryKeyStore;
    use std::{
        sync::atomic::{AtomicBool, AtomicUsize, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    struct Fake {
        version: Option<String>,
        fail_download: AtomicBool,
        fail_install: AtomicBool,
        installs: AtomicUsize,
        downloads: AtomicUsize,
        install_saw_marker: AtomicBool,
        store: Mutex<Option<Arc<Store>>>,
        gate: Mutex<Option<Arc<Mutex<()>>>>,
        gate_held_in_install: AtomicBool,
    }
    impl UpdateSource for Fake {
        fn check(&self) -> Result<Option<UpdateInfo>, UpdateSourceError> {
            Ok(self.version.as_ref().map(|v| UpdateInfo {
                version: v.clone(),
                notes: "notes".into(),
            }))
        }
        fn download(
            &self,
            _: &UpdateInfo,
            _: &dyn Fn(u64, Option<u64>),
        ) -> Result<Vec<u8>, UpdateSourceError> {
            self.downloads.fetch_add(1, Ordering::SeqCst);
            if self.fail_download.load(Ordering::SeqCst) {
                Err(UpdateSourceError::Signature("signature rejected".into()))
            } else {
                Ok(vec![1])
            }
        }
        fn install(&self, _: &UpdateInfo, _: &[u8]) -> Result<(), UpdateSourceError> {
            self.installs.fetch_add(1, Ordering::SeqCst);
            if let Some(gate) = self.gate.lock().unwrap().as_ref() {
                self.gate_held_in_install
                    .store(gate.try_lock().is_err(), Ordering::SeqCst);
            }
            if let Some(store) = self.store.lock().unwrap().as_ref() {
                self.install_saw_marker.store(
                    store.get_setting(PENDING_UPDATE_INSTALL).unwrap().is_some(),
                    Ordering::SeqCst,
                );
            }
            if self.fail_install.load(Ordering::SeqCst) {
                Err(UpdateSourceError::Other("install failed".into()))
            } else {
                Ok(())
            }
        }
    }
    fn fake(version: Option<&str>) -> Arc<Fake> {
        Arc::new(Fake {
            version: version.map(str::to_owned),
            fail_download: AtomicBool::new(false),
            fail_install: AtomicBool::new(false),
            installs: AtomicUsize::new(0),
            downloads: AtomicUsize::new(0),
            install_saw_marker: AtomicBool::new(false),
            store: Mutex::new(None),
            gate: Mutex::new(None),
            gate_held_in_install: AtomicBool::new(false),
        })
    }
    fn store() -> Arc<Store> {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        Arc::new(
            Store::open(
                &std::env::temp_dir().join(format!("scribular-update-{id}.db")),
                &MemoryKeyStore::with_key([9; 32]),
            )
            .unwrap(),
        )
    }
    fn manager(
        fake: Arc<Fake>,
        busy: Arc<AtomicBool>,
    ) -> (
        UpdateManager,
        Arc<AtomicUsize>,
        Arc<Mutex<Vec<UpdateState>>>,
    ) {
        let restarts = Arc::new(AtomicUsize::new(0));
        let events = Arc::new(Mutex::new(Vec::new()));
        let r = Arc::clone(&restarts);
        let e = Arc::clone(&events);
        let b = Arc::clone(&busy);
        (
            UpdateManager::new(
                fake,
                Arc::new(move || b.load(Ordering::SeqCst)),
                Arc::new(Mutex::new(())),
                Arc::new(move |s| e.lock().unwrap().push(s)),
                Arc::new(move || {
                    r.fetch_add(1, Ordering::SeqCst);
                }),
            ),
            restarts,
            events,
        )
    }
    #[test]
    fn newer_update_downloads_and_exposes_installable_only_when_ready() {
        let fake = fake(Some("0.2.0"));
        let (manager, _, events) = manager(fake, Arc::new(AtomicBool::new(false)));
        manager.check();
        assert!(
            matches!(manager.state(), UpdateState::Ready { ref version, installable: true, .. } if version == "0.2.0")
        );
        let events = events.lock().unwrap();
        assert_eq!(events[0], UpdateState::Checking { installable: false });
        assert!(matches!(
            events[1],
            UpdateState::Downloading {
                installable: false,
                ..
            }
        ));
        assert!(matches!(
            events[2],
            UpdateState::Ready {
                installable: true,
                ..
            }
        ));
    }
    #[test]
    fn same_or_older_update_is_not_downloaded() {
        for version in ["0.1.0", "0.0.9"] {
            let fake = fake(Some(version));
            let (manager, _, _) = manager(Arc::clone(&fake), Arc::new(AtomicBool::new(false)));
            manager.check();
            assert_eq!(manager.state(), UpdateState::Idle { installable: false });
            assert_eq!(fake.downloads.load(Ordering::SeqCst), 0);
        }
    }
    #[test]
    fn no_update_and_signature_failure_leave_no_staged_bytes() {
        let none = fake(None);
        let (updates, _, _) = manager(none, Arc::new(AtomicBool::new(false)));
        updates.check();
        assert_eq!(updates.state(), UpdateState::Idle { installable: false });
        let fake = fake(Some("0.2.0"));
        fake.fail_download.store(true, Ordering::SeqCst);
        let (updates, _, events) = manager(Arc::clone(&fake), Arc::new(AtomicBool::new(false)));
        updates.check();
        assert!(matches!(updates.state(), UpdateState::Error { .. }));
        assert!(updates.staged.lock().unwrap().is_none());
        assert!(!events
            .lock()
            .unwrap()
            .iter()
            .any(|s| matches!(s, UpdateState::Ready { .. })));
        fake.fail_download.store(false, Ordering::SeqCst);
        updates.check();
        assert!(matches!(updates.state(), UpdateState::Ready { .. }));
        assert_eq!(fake.downloads.load(Ordering::SeqCst), 2);
    }
    #[test]
    fn busy_install_is_refused_and_install_error_does_not_restart() {
        let fake = fake(Some("0.2.0"));
        fake.fail_install.store(true, Ordering::SeqCst);
        let busy = Arc::new(AtomicBool::new(false));
        let (manager, restarts, _) = manager(Arc::clone(&fake), Arc::clone(&busy));
        manager.check();
        for busy_work in [
            "recording",
            "finalizing",
            "queued summary",
            "running summary",
            "model download",
        ] {
            busy.store(true, Ordering::SeqCst);
            assert_eq!(manager.install(), Err("busy".into()), "{busy_work}");
        }
        assert_eq!(fake.installs.load(Ordering::SeqCst), 0);
        busy.store(false, Ordering::SeqCst);
        assert!(manager.install().is_err());
        assert_eq!(restarts.load(Ordering::SeqCst), 0);
        assert!(matches!(manager.state(), UpdateState::Ready { .. }));
    }

    #[test]
    fn readiness_ticker_emits_only_when_busy_changes() {
        let fake = fake(Some("0.2.0"));
        let busy = Arc::new(AtomicBool::new(true));
        let (manager, _, events) = manager(fake, Arc::clone(&busy));
        manager.check();
        let before = events.lock().unwrap().len();
        busy.store(false, Ordering::SeqCst);
        manager.refresh_installable();
        manager.refresh_installable();
        let events = events.lock().unwrap();
        assert_eq!(events.len(), before + 1);
        assert!(matches!(
            events.last(),
            Some(UpdateState::Ready {
                installable: true,
                ..
            })
        ));
    }

    #[test]
    fn marker_is_written_before_install_and_exit_install_respects_busy() {
        let db = store();
        let fake = fake(Some("0.2.0"));
        *fake.store.lock().unwrap() = Some(Arc::clone(&db));
        let busy = Arc::new(AtomicBool::new(false));
        let (manager, restarts, _) = manager(Arc::clone(&fake), Arc::clone(&busy));
        manager.set_store(Arc::clone(&db));
        manager.check();
        manager.install_on_quit();
        assert!(fake.install_saw_marker.load(Ordering::SeqCst));
        let marker: PendingInstall =
            serde_json::from_str(&db.get_setting(PENDING_UPDATE_INSTALL).unwrap().unwrap())
                .unwrap();
        assert_eq!(
            (marker.from.as_str(), marker.to.as_str(), marker.attempts),
            (env!("CARGO_PKG_VERSION"), "0.2.0", 0)
        );
        assert_eq!(restarts.load(Ordering::SeqCst), 0);
        busy.store(true, Ordering::SeqCst);
        manager.install_on_quit();
        assert_eq!(fake.installs.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn install_gate_remains_held_during_install_and_restart() {
        let fake = fake(Some("0.2.0"));
        let gate = Arc::new(Mutex::new(()));
        *fake.gate.lock().unwrap() = Some(Arc::clone(&gate));
        let restart_gate = Arc::clone(&gate);
        let restart_held = Arc::new(AtomicBool::new(false));
        let seen = Arc::clone(&restart_held);
        let updates = UpdateManager::new(
            fake.clone(),
            Arc::new(|| false),
            gate,
            Arc::new(|_| {}),
            Arc::new(move || {
                seen.store(restart_gate.try_lock().is_err(), Ordering::SeqCst);
            }),
        );
        updates.check();
        updates.install().unwrap();
        assert!(fake.gate_held_in_install.load(Ordering::SeqCst));
        assert!(restart_held.load(Ordering::SeqCst));
    }

    #[test]
    fn marker_counts_failed_launches_and_skips_after_three_but_clears_after_success() {
        let db = store();
        let updates = manager(fake(None), Arc::new(AtomicBool::new(false))).0;
        db.set_setting(
            PENDING_UPDATE_INSTALL,
            r#"{"from":"0.1.0","to":"0.2.0","attempts":2}"#,
        )
        .unwrap();
        updates.reconcile_marker_at(&db, "0.1.0");
        assert_eq!(
            db.get_setting(SKIPPED_UPDATE_VERSION).unwrap().as_deref(),
            Some("0.2.0")
        );
        assert_eq!(db.get_setting(PENDING_UPDATE_INSTALL).unwrap(), None);
        db.set_setting(
            PENDING_UPDATE_INSTALL,
            r#"{"from":"0.1.0","to":"0.2.0","attempts":1}"#,
        )
        .unwrap();
        updates.reconcile_marker_at(&db, "0.2.0");
        assert_eq!(db.get_setting(PENDING_UPDATE_INSTALL).unwrap(), None);
        let skipped = manager(fake(Some("0.2.0")), Arc::new(AtomicBool::new(false))).0;
        skipped.set_store(Arc::clone(&db));
        skipped.check();
        assert_eq!(skipped.state(), UpdateState::Idle { installable: false });
        let newer_fake = fake(Some("0.2.1"));
        let newer = manager(Arc::clone(&newer_fake), Arc::new(AtomicBool::new(false))).0;
        newer.set_store(db);
        newer.check();
        assert_eq!(newer_fake.downloads.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn failed_install_on_quit_logs_and_returns_without_restart() {
        let fake = fake(Some("0.2.0"));
        fake.fail_install.store(true, Ordering::SeqCst);
        let (manager, restarts, _) = manager(Arc::clone(&fake), Arc::new(AtomicBool::new(false)));
        manager.check();
        manager.install_on_quit();
        assert_eq!(restarts.load(Ordering::SeqCst), 0);
        assert!(matches!(
            manager.state(),
            UpdateState::Ready {
                installable: true,
                ..
            }
        ));
    }

    #[test]
    fn quit_install_skips_when_gate_is_held() {
        let fake = fake(Some("0.2.0"));
        let gate = Arc::new(Mutex::new(()));
        let (manager, _, _) = manager(Arc::clone(&fake), Arc::new(AtomicBool::new(false)));
        let manager = UpdateManager {
            gate: Arc::clone(&gate),
            ..manager
        };
        manager.check();

        let acquired = Arc::new(std::sync::Barrier::new(2));
        let release = Arc::new(std::sync::Barrier::new(2));
        let thread_gate = Arc::clone(&gate);
        let thread_acquired = Arc::clone(&acquired);
        let thread_release = Arc::clone(&release);
        let holder = std::thread::spawn(move || {
            let _guard = thread_gate.lock().unwrap();
            thread_acquired.wait();
            thread_release.wait();
        });
        acquired.wait();
        manager.install_on_quit();
        assert_eq!(fake.installs.load(Ordering::SeqCst), 0);
        release.wait();
        holder.join().unwrap();
    }

    #[test]
    fn successful_install_clears_ready_state_before_following_quit_install() {
        let fake = fake(Some("0.2.0"));
        let (manager, restarts, _) = manager(Arc::clone(&fake), Arc::new(AtomicBool::new(false)));
        manager.check();
        manager.install().unwrap();
        assert_eq!(manager.state(), UpdateState::Idle { installable: false });
        assert!(manager.staged.lock().unwrap().is_none());

        manager.install_on_quit();
        assert_eq!(fake.installs.load(Ordering::SeqCst), 1);
        assert_eq!(restarts.load(Ordering::SeqCst), 1);
    }
}
