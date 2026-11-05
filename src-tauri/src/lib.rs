pub mod audio;
pub mod commands;
pub mod engines;
pub mod jobs;
pub mod license;
pub mod llm;
pub mod models;
pub mod store;
pub mod stt;
pub mod text;

use std::{sync::Arc, time::Duration};

use tauri::{Emitter, Manager};

use audio::{AudioCapCli, CpalMicrophone};
use commands::{Heartbeat, LockedState, TauriJobEvents, HEARTBEAT_EVENT};
use jobs::{JobQueue, Recorder};
use llm::SummarizerLoader;
use models::ModelManager;
use store::{key::KeyStore, Store, StoreError};
use stt::TranscriberLoader;

/// Keychain account holding the database key; the service is the bundle id.
const DB_KEY_ACCOUNT: &str = "database-key";

#[cfg(target_os = "macos")]
fn key_store(service: &str, account: &str) -> Arc<dyn KeyStore> {
    Arc::new(store::key::KeychainKeyStore::new(service, account))
}

// shortcut: no persistent key store off macOS (the app is macOS-only); a
// relaunch there shows the locked screen. Add one if another OS ships.
#[cfg(not(target_os = "macos"))]
fn key_store(_service: &str, _account: &str) -> Arc<dyn KeyStore> {
    Arc::new(store::key::MemoryKeyStore::default())
}

#[cfg(feature = "fake-engines")]
fn summarizer_loader(_models: &Arc<ModelManager>) -> Arc<dyn SummarizerLoader> {
    Arc::new(engines::fake::FakeLoader::new(
        engines::fake::FakeSummarizer::new(8192),
    ))
}

#[cfg(not(feature = "fake-engines"))]
fn summarizer_loader(models: &Arc<ModelManager>) -> Arc<dyn SummarizerLoader> {
    Arc::new(llm::llama::LlamaLoader::new(Arc::clone(models)))
}

#[cfg(feature = "fake-engines")]
fn transcriber_loader(_models: &Arc<ModelManager>) -> Arc<dyn TranscriberLoader> {
    Arc::new(engines::fake::FakeTranscriberLoader(Arc::new(
        engines::fake::FakeTranscriber::new(Vec::new()),
    )))
}

#[cfg(not(feature = "fake-engines"))]
fn transcriber_loader(models: &Arc<ModelManager>) -> Arc<dyn TranscriberLoader> {
    Arc::new(stt::whisper::WhisperLoader::new(Arc::clone(models)))
}

/// R6/R12: re-validate at launch (never blocking it) and daily, raise the
/// clock high-water mark hourly, and emit whenever the state changes, which
/// also covers the day rollover.
fn spawn_license_checks(app: tauri::AppHandle, licensing: Arc<license::Licensing>) {
    const TICK: Duration = Duration::from_secs(60 * 60);
    const REVALIDATE_EVERY: u32 = 24;
    std::thread::spawn(move || {
        let mut last = None;
        for tick in (0..REVALIDATE_EVERY).cycle() {
            if tick == 0 {
                licensing.revalidate();
            }
            licensing.touch();
            let state = licensing.state();
            if last.as_ref() != Some(&state) {
                commands::emit_access(&app, &licensing);
                last = Some(state);
            }
            std::thread::sleep(TICK);
        }
    });
}

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::app_version,
            commands::list_sessions,
            commands::get_session,
            commands::update_session,
            commands::delete_session,
            commands::list_prompts,
            commands::list_models,
            commands::models_status,
            commands::download_models,
            commands::regenerate_summary,
            commands::cancel_job,
            commands::transcribe_file,
            commands::get_recording_settings,
            commands::set_recording_settings,
            commands::list_audio_apps,
            commands::start_recording,
            commands::stop_recording,
            commands::current_recording,
            commands::open_privacy_settings,
            commands::request_session_audio,
            commands::locked_state,
            commands::start_fresh,
            commands::quit_app,
            commands::take_encryption_notice,
            commands::get_access_state,
            commands::get_masked_license_key,
            commands::activate_license,
            commands::deactivate_license,
            commands::open_external_link
        ])
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let db_path = data_dir.join("scribular.db");
            let keys = key_store(&app.config().identifier, DB_KEY_ACCOUNT);
            let opened = Store::open(&db_path, keys.as_ref());
            let reason = match &opened {
                Err(StoreError::Locked(reason)) => Some(*reason),
                _ => None,
            };
            app.manage(LockedState {
                reason,
                db_path,
                keys,
            });
            if reason.is_some() {
                // R7: no Store, queues or recorder; the webview shows only the
                // locked screen and its commands.
                return Ok(());
            }
            let store = Arc::new(opened?);
            app.manage(Arc::clone(&store));
            jobs::spawn_retention(&store, jobs::SWEEP_EVERY, store::now_ms);

            let licensing = Arc::new(
                license::Licensing::init(
                    Arc::clone(&store),
                    key_store(&app.config().identifier, license::KEYCHAIN_ACCOUNT),
                    Arc::new(license::api::HttpLicenseApi::new(&license::api::api_root())?),
                    Arc::new(license::SystemClock),
                )
                .with_skip(license::dev_skip()),
            );
            app.manage(Arc::clone(&licensing));
            spawn_license_checks(app.handle().clone(), licensing);

            let models = Arc::new(ModelManager::new(
                data_dir.join("models"),
                models::manifest(),
            ));
            app.manage(Arc::clone(&models));
            let stt = transcriber_loader(&models);
            app.manage(Arc::clone(&stt));
            let events = Arc::new(TauriJobEvents(app.handle().clone()));
            let jobs = Arc::new(JobQueue::new(
                Arc::clone(&store),
                summarizer_loader(&models),
                events.clone(),
            ));
            app.manage(Arc::clone(&jobs));
            // Audio capture stays real under `fake-engines`; only the models
            // are faked.
            app.manage(Arc::new(Recorder::new(
                store,
                jobs,
                stt,
                events,
                Arc::new(CpalMicrophone),
                Arc::new(AudioCapCli::bundled()?),
            )));
            std::thread::spawn(move || models.scan());

            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let mut seq = 0u32;
                loop {
                    std::thread::sleep(Duration::from_secs(1));
                    seq = seq.wrapping_add(1);
                    let _ = handle.emit(HEARTBEAT_EVENT, Heartbeat { seq });
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    //! KTD10: every registered command and emitted event has a wrapper in
    //! `src/lib/api.ts`, and the wrappers name nothing that isn't registered.

    const LIB_RS: &str = include_str!("lib.rs");
    const API_TS: &str = include_str!("../../src/lib/api.ts");

    fn registered_commands() -> Vec<&'static str> {
        let start = LIB_RS
            .find("generate_handler![")
            .expect("generate_handler! not found")
            + "generate_handler![".len();
        let end = start + LIB_RS[start..].find(']').unwrap();
        LIB_RS[start..end]
            .split(',')
            .map(|p| p.trim().rsplit("::").next().unwrap().trim())
            .filter(|n| !n.is_empty())
            .collect()
    }

    /// The first quoted argument of each `invoke(` / `listen(` call in api.ts.
    fn api_names(call: &str) -> Vec<String> {
        API_TS
            .match_indices(call)
            .filter_map(|(i, _)| {
                let rest = &API_TS[i + call.len()..];
                let rest = match rest.strip_prefix('<') {
                    Some(generic) => &generic[generic.find('>')? + 1..],
                    None => rest,
                };
                let rest = rest.strip_prefix("('")?;
                Some(rest[..rest.find('\'')?].to_string())
            })
            .collect()
    }

    #[test]
    fn every_command_has_an_api_wrapper() {
        let wrapped = api_names("invoke");
        let registered = registered_commands();
        assert!(!registered.is_empty());
        for cmd in &registered {
            assert!(
                wrapped.iter().any(|w| w == cmd),
                "command `{cmd}` has no invoke wrapper in src/lib/api.ts"
            );
        }
        for w in &wrapped {
            assert!(
                registered.contains(&w.as_str()),
                "src/lib/api.ts invokes `{w}`, which is not registered"
            );
        }
    }

    #[test]
    fn every_event_has_an_api_listener() {
        let listened = api_names("listen");
        for event in super::commands::EVENTS {
            assert!(
                listened.iter().any(|l| l == event),
                "event `{event}` has no listen wrapper in src/lib/api.ts"
            );
        }
    }
}
