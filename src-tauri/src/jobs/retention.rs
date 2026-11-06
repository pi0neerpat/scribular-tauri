//! Kept-audio retention (parity spec 01 R17): sweep at start, then hourly
//! while the app runs, so audio expires without a relaunch.

use std::{
    sync::{Arc, Weak},
    thread::JoinHandle,
    time::Duration,
};

use crate::store::Store;

pub const SWEEP_EVERY: Duration = Duration::from_secs(60 * 60);

/// Sweeps now, then every `every`, reading the time from `clock` (injectable
/// for tests). Stops once the store is dropped.
pub fn spawn_retention(
    store: &Arc<Store>,
    every: Duration,
    clock: impl Fn() -> i64 + Send + 'static,
) -> JoinHandle<()> {
    let store: Weak<Store> = Arc::downgrade(store);
    std::thread::spawn(move || loop {
        let Some(s) = store.upgrade() else { return };
        if let Err(e) = s.sweep_expired_audio(clock()) {
            crate::diag::log::log(
                "ERROR",
                "retention",
                &format!(
                    "audio retention sweep failed ({})",
                    crate::diag::log::kind(&e)
                ),
            );
        }
        drop(s);
        std::thread::sleep(every);
    })
}
