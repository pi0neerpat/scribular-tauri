pub mod audio;
pub mod commands;
pub mod engines;
pub mod jobs;
pub mod llm;
pub mod models;
pub mod store;
pub mod stt;
pub mod text;

use std::{sync::Arc, time::Duration};

use tauri::{Emitter, Manager};

use audio::{AudioCapCli, CpalMicrophone};
use commands::{Heartbeat, TauriJobEvents, HEARTBEAT_EVENT};
use jobs::{JobQueue, Recorder};
use llm::SummarizerLoader;
use models::ModelManager;
use store::Store;
use stt::TranscriberLoader;

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
            commands::open_privacy_settings
        ])
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let store = Arc::new(Store::open(&data_dir.join("scribular.db"))?);
            app.manage(Arc::clone(&store));

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
