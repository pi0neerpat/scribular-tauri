use std::{
    str::FromStr,
    sync::Mutex,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use ts_rs::TS;

use crate::{
    shell::{
        menu::BINDINGS,
        recording::{background, BackgroundAction},
    },
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
    pending: Vec<String>,
    error: Option<String>,
    capturing: bool,
    press: PressGuard,
}

#[derive(Default)]
struct PressGuard {
    held: bool,
    last: Option<Instant>,
}

impl PressGuard {
    fn accepts(&mut self, state: ShortcutState, now: Instant) -> bool {
        if state == ShortcutState::Released {
            self.held = false;
            return false;
        }
        if self.held {
            return false;
        }
        self.held = true;
        if self
            .last
            .is_some_and(|last| now.duration_since(last) < Duration::from_secs(1))
        {
            return false;
        }
        self.last = Some(now);
        true
    }
}

impl Actual {
    fn known_active(&self) -> Option<&str> {
        self.active.as_deref().filter(|_| self.pending.is_empty())
    }
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
        active_chord: actual.known_active().map(str::to_owned),
        error: actual.error.clone(),
    })
}

pub fn active(app: &AppHandle) -> Option<String> {
    let registration = app.try_state::<Registration>()?;
    let actual = registration.0.try_lock().ok()?;
    actual.known_active().map(str::to_owned)
}

pub fn toggle_on_press(app: &AppHandle, shortcut: &Shortcut, state: ShortcutState) {
    let Some(registration) = app.try_state::<Registration>() else {
        return;
    };
    // Registration changes can hold this lock while waiting for the main thread.
    let Ok(mut actual) = registration.0.try_lock() else {
        return;
    };
    if actual.known_active() != Some(shortcut.to_string().as_str()) {
        return;
    }
    if !actual.press.accepts(state, Instant::now()) {
        return;
    }
    drop(actual);
    background(app, BackgroundAction::Toggle);
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
    actual.pending.retain(|chord| unregister(chord).is_err());
    if !actual.pending.is_empty() {
        return Err("Could not release the previous shortcut. Try again.".into());
    }
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
                    actual.pending.push(chord.to_owned());
                }
            }
            return Err("Could not release the previous shortcut. Try again.".into());
        }
    }
    actual.active = next.map(str::to_owned);
    actual.press.held = false;
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
        drop(actual);
        if let Ok(result) = state(app, store) {
            let _ = app.emit(REGISTRATION_EVENT, result);
        }
        refresh_tray(app);
        return Err(error);
    }
    if store.set_shortcut_settings(&settings).is_err() {
        let previous = desired(&current, false);
        let message = match replace(app, &mut actual, previous) {
            Ok(()) => "Could not save shortcut settings.".to_string(),
            Err(rollback) => format!("Could not save shortcut settings. {rollback}"),
        };
        actual.error = Some(message.clone());
        drop(actual);
        if let Ok(result) = state(app, store) {
            let _ = app.emit(REGISTRATION_EVENT, result);
        }
        refresh_tray(app);
        return Err(message);
    }
    actual.capturing = false;
    actual.error = None;
    drop(actual);
    let result = state(app, store)?;
    let _ = app.emit(REGISTRATION_EVENT, &result);
    refresh_tray(app);
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
    refresh_tray(app);
    let result = state(app, store)?;
    let _ = app.emit(REGISTRATION_EVENT, &result);
    Ok(result)
}

pub fn refresh_tray(app: &AppHandle) {
    let (recording, multi_speaker) = app
        .try_state::<std::sync::Arc<crate::jobs::Recorder>>()
        .map_or((false, false), |recorder| recorder.tray_state());
    crate::shell::tray::refresh(app, recording, multi_speaker);
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
        assert_eq!(
            Shortcut::from_str("control+alt+super+KeyR")
                .unwrap()
                .to_string(),
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
        assert_eq!(actual.active.as_deref(), Some("old"));
        assert_eq!(actual.pending, vec!["new"]);
        assert_eq!(actual.known_active(), None);
    }

    #[test]
    fn dual_unregister_failure_retries_both_chords() {
        let mut actual = Actual {
            active: Some("old".into()),
            ..Default::default()
        };
        let registered = std::cell::RefCell::new(
            ["old".to_string()]
                .into_iter()
                .collect::<std::collections::HashSet<_>>(),
        );
        let mut attempts = Vec::new();
        assert!(replace_with(
            &mut actual,
            Some("new"),
            |chord| {
                registered.borrow_mut().insert(chord.into());
                Ok(())
            },
            |chord| {
                attempts.push(chord.to_string());
                Err("busy".into())
            },
        )
        .is_err());
        assert_eq!(actual.active.as_deref(), Some("old"));
        assert_eq!(actual.pending, vec!["new"]);
        assert_eq!(attempts, ["old", "new"]);

        attempts.clear();
        assert!(replace_with(
            &mut actual,
            None,
            |_| Ok(()),
            |chord| {
                attempts.push(chord.to_string());
                Err("busy".into())
            },
        )
        .is_err());
        assert_eq!(attempts, ["new"]);
        assert_eq!(actual.pending, vec!["new"]);
        assert_eq!(actual.active.as_deref(), Some("old"));

        attempts.clear();
        assert!(replace_with(
            &mut actual,
            None,
            |_| Ok(()),
            |chord| {
                attempts.push(chord.to_string());
                registered.borrow_mut().remove(chord);
                Ok(())
            },
        )
        .is_ok());
        assert_eq!(attempts, ["new", "old"]);
        assert!(registered.borrow().is_empty());
        assert!(actual.pending.is_empty());
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

    #[test]
    fn press_guard_ignores_repeat_release_and_rapid_second_press() {
        let now = Instant::now();
        let mut guard = PressGuard::default();
        assert!(guard.accepts(ShortcutState::Pressed, now));
        assert!(!guard.accepts(ShortcutState::Pressed, now));
        assert!(!guard.accepts(ShortcutState::Released, now));
        assert!(!guard.accepts(ShortcutState::Pressed, now + Duration::from_millis(500)));
        assert!(!guard.accepts(ShortcutState::Released, now));
        assert!(guard.accepts(ShortcutState::Pressed, now + Duration::from_secs(1)));
    }
}
