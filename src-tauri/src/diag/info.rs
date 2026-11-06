use std::{path::Path, process::Command};

use crate::{
    models::{ModelManager, ModelState},
    store::Store,
};

pub fn diagnostics(store: &Store, models: &ModelManager, log_dir: &Path) -> Result<String, String> {
    let version = env!("CARGO_PKG_VERSION");
    let build = option_env!("GIT_SHA")
        .map(|sha| format!(" (build {sha})"))
        .unwrap_or_default();
    let os = Command::new("sw_vers")
        .arg("-productVersion")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".into());
    let arch = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        std::env::consts::ARCH
    };
    let schema = store
        .schema_version()
        .map_err(|_| "could not read database version".to_string())?;
    let telemetry = match store
        .get_setting(crate::diag::telemetry::TELEMETRY_ENABLED)
        .map_err(|_| "could not read telemetry setting".to_string())?
        .as_deref()
    {
        Some("1") => "on",
        Some("0") => "off",
        _ => "undecided",
    };
    let models = models
        .status()
        .into_iter()
        .map(|m| {
            let state = match m.state {
                ModelState::Missing => "missing",
                ModelState::Downloading => "downloading",
                ModelState::Verifying => "verifying",
                ModelState::Ready => "ready",
                ModelState::Failed => "failed",
            };
            format!("{}:{state}", m.model_id)
        })
        .collect::<Vec<_>>()
        .join(", ");
    let dir = std::env::var_os("HOME")
        .and_then(|home| {
            log_dir
                .strip_prefix(home)
                .ok()
                .map(|rest| format!("~/{}", rest.display()))
        })
        .unwrap_or_else(|| log_dir.display().to_string());
    Ok(format!("Scribular {version}{build}\nmacOS {os} ({arch})\nTauri {}\nDB schema v{schema}\nModels: {models}\nTelemetry: {telemetry}\nLog dir: {dir}", tauri::VERSION))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostics_are_seven_content_free_lines_with_tilde_home() {
        let base = std::env::temp_dir().join(format!("scribular-info-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let store = Store::open(
            &base.join("db"),
            &crate::store::key::MemoryKeyStore::with_key([5; 32]),
        )
        .unwrap();
        let models = ModelManager::new(base.join("models"), crate::models::manifest());
        let home = std::env::var("HOME").unwrap();
        let log_dir =
            Path::new(&home).join("Library/Application Support/com.app.scribular.tauri/logs");
        let text = diagnostics(&store, &models, &log_dir).unwrap();
        assert_eq!(text.lines().count(), 7);
        assert!(text.starts_with("Scribular "));
        assert!(
            text.contains("Log dir: ~/Library/Application Support/com.app.scribular.tauri/logs"),
            "{text}"
        );
        assert!(!text.contains(&home));
        std::fs::remove_dir_all(base).unwrap();
    }
}
