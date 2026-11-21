use std::{str::FromStr, sync::Mutex};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Modifiers, Shortcut};
use ts_rs::TS;

use crate::{
    shell::menu::BINDINGS,
    store::{ShortcutSettings, Store},
};

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutRegistrationState {
    pub settings: ShortcutSettings,
    pub active_chord: Option<String>,
    pub error: Option<String>,
}

pub const REGISTRATION_EVENT: &str = "shortcut://registration";

#[derive(Default)]
pub struct Registration(Mutex<Actual>);

#[derive(Default)]
struct Actual {
    active: Option<String>,
    error: Option<String>,
    capturing: bool,
}

pub fn validate(chord: &str) -> Result<String, String> {
    let key =
        Shortcut::from_str(chord).map_err(|_| "Choose a letter, digit, or F-key.".to_string())?;
    let name = key.key.to_string();
    let allowed_key = name.starts_with("Key") && name.len() == 4
        || name.starts_with("Digit") && name.len() == 6
        || name
            .strip_prefix('F')
            .and_then(|n| n.parse::<u8>().ok())
            .is_some_and(|n| (1..=24).contains(&n));
    let mods = key.mods;
    if !allowed_key
        || !mods.intersects(Modifiers::SUPER | Modifiers::CONTROL)
        || (mods & (Modifiers::SUPER | Modifiers::CONTROL | Modifiers::ALT | Modifiers::SHIFT))
            .bits()
            .count_ones()
            < 2
    {
        return Err("Use ⌘ or ⌃ plus another modifier and a letter, digit, or F-key.".into());
    }
    if BINDINGS
        .iter()
        .filter_map(|(_, value)| Shortcut::from_str(value).ok())
        .any(|binding| binding == key)
    {
        return Err("That key is already used by a Scribular menu command.".into());
    }
    // macOS owns these screenshot, session, and system control chords.
    if [
        "Super+Space",
        "Ctrl+Space",
        "Super+Tab",
        "Super+Q",
        "Super+H",
        "Ctrl+Super+Q",
        "Ctrl+F2",
        "Ctrl+F3",
        "Shift+Super+3",
        "Shift+Super+4",
        "Shift+Super+5",
        "Shift+Super+6",
        "Ctrl+Shift+Super+3",
        "Ctrl+Shift+Super+4",
        "Shift+Super+Q",
    ]
    .iter()
    .filter_map(|value| Shortcut::from_str(value).ok())
    .any(|reserved| reserved == key)
    {
        return Err("That key is reserved by macOS.".into());
    }
    Ok(key.into_string())
}

pub fn state(app: &AppHandle, store: &Store) -> Result<ShortcutRegistrationState, String> {
    let settings = store.shortcut_settings().map_err(|e| e.to_string())?;
    let actual = app.state::<Registration>();
    let actual = actual.0.lock().unwrap_or_else(|e| e.into_inner());
    Ok(ShortcutRegistrationState {
        settings,
        active_chord: actual.active.clone(),
        error: actual.error.clone(),
    })
}

pub fn active(app: &AppHandle) -> Option<String> {
    app.try_state::<Registration>()
        .and_then(|state| state.0.try_lock().ok().and_then(|s| s.active.clone()))
}

fn replace(app: &AppHandle, actual: &mut Actual, next: Option<&str>) -> Result<(), String> {
    replace_with(
        actual,
        next,
        |chord| {
            app.global_shortcut()
                .register(chord)
                .map_err(|_| "macOS could not register that shortcut. Try another key.".to_string())
        },
        |chord| {
            app.global_shortcut()
                .unregister(chord)
                .map_err(|_| "Could not release the previous shortcut. Try again.".to_string())
        },
    )
}

fn replace_with(
    actual: &mut Actual,
    next: Option<&str>,
    mut register: impl FnMut(&str) -> Result<(), String>,
    mut unregister: impl FnMut(&str) -> Result<(), String>,
) -> Result<(), String> {
    if actual.active.as_deref() == next {
        return Ok(());
    }
    if let Some(chord) = next {
        register(chord)?;
    }
    if let Some(old) = &actual.active {
        if unregister(old).is_err() {
            if let Some(chord) = next {
                if unregister(chord).is_err() {
                    actual.active = None;
                }
            }
            return Err("Could not release the previous shortcut. Try again.".into());
        }
    }
    actual.active = next.map(str::to_owned);
    Ok(())
}

fn desired(settings: &ShortcutSettings, capturing: bool) -> Option<&str> {
    if settings.global_shortcuts_enabled && !capturing {
        settings.toggle_recording.as_deref()
    } else {
        None
    }
}

pub fn set(
    app: &AppHandle,
    store: &Store,
    mut settings: ShortcutSettings,
) -> Result<ShortcutRegistrationState, String> {
    settings.toggle_recording = settings
        .toggle_recording
        .as_deref()
        .map(validate)
        .transpose()?;
    let current = store.shortcut_settings().map_err(|e| e.to_string())?;
    let registration = app.state::<Registration>();
    let mut actual = registration.0.lock().unwrap_or_else(|e| e.into_inner());
    let next = desired(&settings, false);
    if let Err(error) = replace(app, &mut actual, next) {
        actual.error = Some(error.clone());
        return Err(error);
    }
    if let Err(error) = store.set_shortcut_settings(&settings) {
        let previous = desired(&current, false);
        let message = match replace(app, &mut actual, previous) {
            Ok(()) => error.to_string(),
            Err(rollback) => {
                let failed = next.is_some_and(|chord| app.global_shortcut().is_registered(chord));
                let old = previous.is_some_and(|chord| app.global_shortcut().is_registered(chord));
                actual.active = if failed && old {
                    None
                } else if failed {
                    next.map(str::to_owned)
                } else if old {
                    previous.map(str::to_owned)
                } else {
                    None
                };
                format!("Could not save shortcut: {error}; could not restore previous shortcut: {rollback}")
            }
        };
        actual.error = Some(message.clone());
        drop(actual);
        if let Ok(result) = state(app, store) {
            let _ = app.emit(REGISTRATION_EVENT, result);
        }
        refresh_tray(app, store);
        return Err(message);
    }
    actual.capturing = false;
    actual.error = None;
    drop(actual);
    let result = state(app, store)?;
    let _ = app.emit(REGISTRATION_EVENT, &result);
    refresh_tray(app, store);
    Ok(result)
}

pub fn capture(
    app: &AppHandle,
    store: &Store,
    capturing: bool,
) -> Result<ShortcutRegistrationState, String> {
    let registration = app.state::<Registration>();
    let mut actual = registration.0.lock().unwrap_or_else(|e| e.into_inner());
    if capturing {
        replace(app, &mut actual, None)?;
        actual.capturing = true;
    } else if actual.capturing {
        let saved = store.shortcut_settings().map_err(|e| e.to_string())?;
        let next = desired(&saved, false);
        if let Err(error) = replace(app, &mut actual, next) {
            actual.error = Some(error);
        }
        actual.capturing = false;
    }
    drop(actual);
    refresh_tray(app, store);
    let result = state(app, store)?;
    let _ = app.emit(REGISTRATION_EVENT, &result);
    Ok(result)
}

pub fn refresh_tray(app: &AppHandle, store: &Store) {
    let recording = app
        .try_state::<std::sync::Arc<crate::jobs::Recorder>>()
        .is_some_and(|r| r.recording_state().is_some());
    let multi = store.recording_settings().is_ok_and(|s| s.multi_speaker);
    crate::shell::tray::refresh(app, recording, multi);
}

pub fn restore(app: &AppHandle, store: &Store) {
    let Ok(settings) = store.shortcut_settings() else {
        return;
    };
    let registration = app.state::<Registration>();
    let mut actual = registration.0.lock().unwrap_or_else(|e| e.into_inner());
    let Some(chord) = desired(&settings, actual.capturing) else {
        return;
    };
    match validate(chord).and_then(|valid| replace(app, &mut actual, Some(&valid))) {
        Ok(()) => actual.error = None,
        Err(error) => actual.error = Some(error),
    }
    drop(actual);
    if let Ok(result) = state(app, store) {
        let _ = app.emit(REGISTRATION_EVENT, result);
    }
}

pub fn unregister(app: &AppHandle) {
    if let Some(registration) = app.try_state::<Registration>() {
        let mut actual = registration.0.lock().unwrap_or_else(|e| e.into_inner());
        let _ = replace(app, &mut actual, None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_chords() {
        assert_eq!(
            validate("Ctrl+Alt+Super+R").unwrap(),
            "control+alt+super+KeyR"
        );
        assert!(validate("Super+R").is_err());
        assert!(validate("Alt+Super+R").is_err());
        assert!(validate("Ctrl+Alt+Up").is_err());
        assert!(validate("Shift+Super+3").is_err());
        assert!(validate("Ctrl+Alt+F5").is_ok());
    }

    #[test]
    fn failed_replacement_keeps_prior_registration() {
        let mut actual = Actual {
            active: Some("old".into()),
            ..Default::default()
        };
        assert!(replace_with(
            &mut actual,
            Some("new"),
            |_| Err("claimed".into()),
            |_| Ok(())
        )
        .is_err());
        assert_eq!(actual.active.as_deref(), Some("old"));
        assert!(replace_with(&mut actual, None, |_| Ok(()), |_| Ok(())).is_ok());
        assert_eq!(actual.active, None);
    }

    #[test]
    fn failed_cleanup_does_not_claim_a_single_active_chord() {
        let mut actual = Actual {
            active: Some("old".into()),
            ..Default::default()
        };
        assert!(
            replace_with(&mut actual, Some("new"), |_| Ok(()), |_| Err("busy".into())).is_err()
        );
        assert_eq!(actual.active, None);
    }

    #[test]
    fn only_enabled_set_chord_outside_capture_is_desired() {
        let mut settings = ShortcutSettings::default();
        assert_eq!(desired(&settings, false), None);
        settings.global_shortcuts_enabled = true;
        assert_eq!(desired(&settings, false), Some("control+alt+super+KeyR"));
        assert_eq!(desired(&settings, true), None);
        settings.toggle_recording = None;
        assert_eq!(desired(&settings, false), None);
    }
}
