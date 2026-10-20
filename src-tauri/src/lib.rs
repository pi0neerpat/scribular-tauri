pub mod commands;
pub mod store;
pub mod text;

use std::time::Duration;

use tauri::{Emitter, Manager};

use commands::{Heartbeat, HEARTBEAT_EVENT};
use store::Store;

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::app_version,
            commands::list_sessions,
            commands::get_session,
            commands::update_session,
            commands::delete_session,
            commands::list_prompts
        ])
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            app.manage(Store::open(&data_dir.join("scribular.db"))?);

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
