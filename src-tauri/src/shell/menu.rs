use std::{collections::HashMap, sync::Mutex};

use tauri::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem as P, Submenu},
    AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder,
};

// Fixed app bindings live here so the menu and later shortcut settings use the same map.
pub const BINDINGS: &[(&str, &str)] = &[
    ("shell-settings", "CmdOrCtrl+,"),
    ("shell-quit", "CmdOrCtrl+Q"),
    ("shell-new-recording", "CmdOrCtrl+N"),
    ("shell-record", "CmdOrCtrl+R"),
    ("shell-pause", "Alt+CmdOrCtrl+R"),
    ("shell-example-style", "Alt+CmdOrCtrl+N"),
    ("shell-copy-note", "Shift+CmdOrCtrl+C"),
    ("shell-close", "CmdOrCtrl+W"),
    ("shell-delete-session", "CmdOrCtrl+Backspace"),
    ("shell-find", "CmdOrCtrl+F"),
    ("shell-note", "CmdOrCtrl+1"),
    ("shell-transcript", "CmdOrCtrl+2"),
    ("shell-audio", "CmdOrCtrl+3"),
    ("shell-manage-styles", "Alt+CmdOrCtrl+S"),
    ("shell-previous", "Alt+CmdOrCtrl+Up"),
    ("shell-next", "Alt+CmdOrCtrl+Down"),
    ("shell-sidebar", "Ctrl+CmdOrCtrl+S"),
    ("shell-main", "CmdOrCtrl+0"),
    ("shell-help", "CmdOrCtrl+Shift+/"),
    ("native-undo", "CmdOrCtrl+Z"),
    ("native-redo", "Shift+CmdOrCtrl+Z"),
    ("native-cut", "CmdOrCtrl+X"),
    ("native-copy", "CmdOrCtrl+C"),
    ("native-paste", "CmdOrCtrl+V"),
    ("native-select-all", "CmdOrCtrl+A"),
    ("native-fullscreen", "Ctrl+CmdOrCtrl+F"),
    ("native-hide", "CmdOrCtrl+H"),
    ("native-hide-others", "Alt+CmdOrCtrl+H"),
    ("native-minimize", "CmdOrCtrl+M"),
];

fn binding(id: &str) -> Option<&'static str> {
    BINDINGS
        .iter()
        .find(|(command, _)| *command == id)
        .map(|(_, key)| *key)
}

fn recording_menu_state(
    recording: bool,
    paused: bool,
    locked: bool,
    writable: bool,
) -> ((&'static str, bool), (&'static str, bool)) {
    (
        (
            if recording {
                "Stop Recording"
            } else {
                "Start Recording"
            },
            recording || writable && !locked,
        ),
        (
            if paused {
                "Resume Recording"
            } else {
                "Pause Recording"
            },
            recording && !locked,
        ),
    )
}

fn note_menu_state(selected: bool, has_note: bool, locked: bool, recording: bool) -> (bool, bool) {
    (!locked && selected && !recording, !locked && has_note)
}

#[derive(Default)]
struct CoachMenuState(Mutex<Option<HashMap<String, bool>>>);

fn set_enabled(
    app: &AppHandle,
    item: &tauri::menu::MenuItem<tauri::Wry>,
    enabled: bool,
) -> tauri::Result<()> {
    let states = app.state::<CoachMenuState>();
    let mut states = states.0.lock().unwrap_or_else(|poison| poison.into_inner());
    if let Some(baseline) = states.as_mut() {
        baseline.insert(item.id().as_ref().to_owned(), enabled);
        item.set_enabled(false)?;
    } else {
        item.set_enabled(enabled)?;
    }
    Ok(())
}

fn overlay_coach_menu(app: &AppHandle, open: bool) {
    let Some(menu) = app.menu() else { return };
    let states = app.state::<CoachMenuState>();
    let mut states = states.0.lock().unwrap_or_else(|poison| poison.into_inner());
    if open {
        if states.is_none() {
            let mut baseline = HashMap::new();
            for top in menu.items().unwrap_or_default() {
                if let tauri::menu::MenuItemKind::Submenu(sub) = top {
                    for entry in sub.items().unwrap_or_default() {
                        if let tauri::menu::MenuItemKind::MenuItem(item) = entry {
                            let id = item.id().as_ref().to_owned();
                            if id.starts_with("shell-")
                                && !matches!(id.as_str(), "shell-close" | "shell-quit")
                            {
                                if let Ok(enabled) = item.is_enabled() {
                                    baseline.insert(id, enabled);
                                }
                            }
                        }
                    }
                }
            }
            *states = Some(baseline);
        }
        for top in menu.items().unwrap_or_default() {
            if let tauri::menu::MenuItemKind::Submenu(sub) = top {
                for entry in sub.items().unwrap_or_default() {
                    if let tauri::menu::MenuItemKind::MenuItem(item) = entry {
                        if item.id().as_ref().starts_with("shell-")
                            && !matches!(item.id().as_ref(), "shell-close" | "shell-quit")
                        {
                            let _ = item.set_enabled(false);
                        }
                    }
                }
            }
        }
    } else if let Some(baseline) = states.take() {
        for top in menu.items().unwrap_or_default() {
            if let tauri::menu::MenuItemKind::Submenu(sub) = top {
                for entry in sub.items().unwrap_or_default() {
                    if let tauri::menu::MenuItemKind::MenuItem(item) = entry {
                        if let Some(enabled) = baseline.get(item.id().as_ref()) {
                            let _ = item.set_enabled(*enabled);
                        }
                    }
                }
            }
        }
    }
}

pub fn open_settings(app: &AppHandle, pane: &str) -> Result<(), String> {
    let pane = match pane {
        "General" | "Appearance" | "Recording" | "Keyboard Shortcuts" | "License" => pane,
        _ => "General",
    };
    if let Some(window) = app.get_webview_window("settings") {
        super::show_window(&window).map_err(|e| e.to_string())?;
        window.set_focus().map_err(|e| e.to_string())?;
        window
            .emit("shell://settings-pane", pane)
            .map_err(|e| e.to_string())?;
        return Ok(());
    }
    let url = WebviewUrl::App(
        format!(
            "index.html?window=settings&pane={}",
            pane.replace(' ', "%20")
        )
        .into(),
    );
    super::restore_dock_icon(app);
    WebviewWindowBuilder::new(app, "settings", url)
        .title("Scribular Settings")
        .inner_size(680.0, 560.0)
        .build()
        .inspect_err(|_| {
            if let Err(e) = super::hide_dock_if_no_windows(app) {
                crate::diag::log::log(
                    "WARN",
                    "shell",
                    &format!("could not hide the Dock icon: {e}"),
                );
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn handle(app: &AppHandle, event: MenuEvent) {
    let id = event.id().as_ref();
    if !id.starts_with("shell-") {
        return;
    }
    if let Some(menu) = app.menu() {
        for top in menu.items().unwrap_or_default() {
            if let tauri::menu::MenuItemKind::Submenu(sub) = top {
                if let Some(tauri::menu::MenuItemKind::MenuItem(item)) = sub.get(id) {
                    if item.is_enabled().ok() != Some(true) {
                        return;
                    }
                }
            }
        }
    }
    if crate::coach::is_open(app) && !matches!(id, "shell-close" | "shell-quit") {
        return;
    }
    if id == "shell-settings" || id == "shell-shortcuts" {
        let pane = if id == "shell-shortcuts" {
            "Keyboard Shortcuts"
        } else {
            "General"
        };
        if let Err(e) = open_settings(app, pane) {
            crate::diag::log::log("WARN", "shell", &e);
        }
        return;
    }
    if id == "shell-quit" {
        app.exit(0);
        return;
    }
    if id == "shell-close" {
        let window = app
            .get_webview_window("settings")
            .filter(|window| window.is_focused().unwrap_or(false))
            .or_else(|| app.get_webview_window("main"));
        if let Some(window) = window {
            let _ = window.close();
        }
        return;
    }
    if let Some(window) = super::show_main(app) {
        let _ = window.emit("shell://menu", id);
    }
}

pub fn set_state(
    app: &AppHandle,
    has_sessions: bool,
    selected: bool,
    has_note: bool,
    recording: bool,
    recoverable: bool,
    locked: bool,
    writable: bool,
    paused: bool,
) -> Result<(), String> {
    let menu = app.menu().ok_or("no app menu")?;
    let (record_state, pause_state) = recording_menu_state(recording, paused, locked, writable);
    let (can_delete, can_copy) = note_menu_state(selected, has_note, locked, recording);
    for (id, title) in [
        ("shell-record", record_state.0),
        ("shell-pause", pause_state.0),
    ] {
        for top in menu.items().map_err(|e| e.to_string())? {
            if let tauri::menu::MenuItemKind::Submenu(sub) = top {
                if let Some(tauri::menu::MenuItemKind::MenuItem(item)) = sub.get(id) {
                    item.set_text(title).map_err(|e| e.to_string())?;
                }
            }
        }
    }
    for (id, enabled) in [
        ("shell-new-recording", writable && !locked && !recording),
        ("shell-record", record_state.1),
        ("shell-pause", pause_state.1),
        ("shell-manage-styles", !locked),
        ("shell-blank-style", writable && !locked && !recording),
        ("shell-find", !locked),
        ("shell-delete-all", !recording && has_sessions),
        ("shell-delete-session", can_delete),
        ("shell-copy-note", can_copy),
        (
            "shell-recover",
            writable && !locked && recoverable && !recording,
        ),
        ("shell-restore-sample", writable && !locked),
        ("shell-previous", !locked && has_sessions),
        ("shell-next", !locked && has_sessions),
        ("shell-sidebar", !locked),
    ] {
        for top in menu.items().map_err(|e| e.to_string())? {
            if let tauri::menu::MenuItemKind::Submenu(sub) = top {
                if let Some(tauri::menu::MenuItemKind::MenuItem(item)) = sub.get(id) {
                    set_enabled(app, &item, enabled).map_err(|e| e.to_string())?;
                }
            }
        }
    }
    Ok(())
}

pub fn sync_recording_state(app: &AppHandle, state: Option<&crate::jobs::RecordingState>) {
    let Some(menu) = app.menu() else { return };
    let recording = state.is_some();
    let paused = state.is_some_and(|state| state.status == crate::jobs::RecordingStatus::Paused);
    let locked = app
        .try_state::<crate::commands::LockedState>()
        .is_none_or(|state| state.reason.is_some());
    let writable = app
        .try_state::<std::sync::Arc<crate::license::Licensing>>()
        .is_some_and(|license| license.require_access().is_ok());
    let (record_state, pause_state) = recording_menu_state(recording, paused, locked, writable);
    for (id, title, enabled) in [
        ("shell-record", record_state.0, record_state.1),
        ("shell-pause", pause_state.0, pause_state.1),
    ] {
        for top in menu.items().unwrap_or_default() {
            if let tauri::menu::MenuItemKind::Submenu(sub) = top {
                if let Some(tauri::menu::MenuItemKind::MenuItem(item)) = sub.get(id) {
                    let _ = item.set_text(title);
                    let _ = set_enabled(app, &item, enabled);
                }
            }
        }
    }
}

pub fn set_coach_state(app: &AppHandle, enabled: bool, open: bool, reason: Option<&str>) {
    overlay_coach_menu(app, open);
    let Some(menu) = app.menu() else { return };
    for (id, title) in [
        ("shell-example-style", "New Style from Example Note…"),
        ("shell-coach", "Make a Note Style from an Example…"),
    ] {
        for top in menu.items().unwrap_or_default() {
            if let tauri::menu::MenuItemKind::Submenu(sub) = top {
                if let Some(tauri::menu::MenuItemKind::MenuItem(item)) = sub.get(id) {
                    let text =
                        reason.map_or_else(|| title.into(), |reason| format!("{title} — {reason}"));
                    let _ = item.set_text(text);
                    let _ = set_enabled(app, &item, enabled);
                }
            }
        }
    }
}

fn item(
    app: &AppHandle,
    menu: &Submenu<tauri::Wry>,
    id: &str,
    title: &str,
    enabled: bool,
    shortcut: Option<&str>,
) -> tauri::Result<()> {
    menu.append(&MenuItem::with_id(
        app,
        id,
        title,
        enabled,
        binding(id).or(shortcut),
    )?)
}

pub fn build(app: &tauri::App) -> tauri::Result<Menu<tauri::Wry>> {
    app.manage(CoachMenuState::default());
    let menu = Menu::new(app)?;
    let scribular = Submenu::new(app, "Scribular", true)?;
    scribular.append(&P::about(app, Some("About Scribular"), None)?)?;
    item(
        &app.handle(),
        &scribular,
        "shell-update",
        "Check for Updates…",
        true,
        None,
    )?;
    item(
        &app.handle(),
        &scribular,
        "shell-settings",
        "Settings…",
        true,
        Some("CmdOrCtrl+,"),
    )?;
    scribular.append(&P::services(app, None)?)?;
    scribular.append(&P::hide(app, None)?)?;
    scribular.append(&P::hide_others(app, None)?)?;
    scribular.append(&P::show_all(app, None)?)?;
    item(
        &app.handle(),
        &scribular,
        "shell-quit",
        "Quit Scribular",
        true,
        Some("CmdOrCtrl+Q"),
    )?;
    menu.append(&scribular)?;

    let file = Submenu::new(app, "File", true)?;
    for (id, title, enabled, key) in [
        (
            "shell-new-recording",
            "New Recording",
            false,
            Some("CmdOrCtrl+N"),
        ),
        ("shell-record", "Start/Stop Recording", false, None),
        ("shell-pause", "Pause/Resume Recording", false, None),
        (
            "shell-example-style",
            "New Style from Example Note…",
            false,
            None,
        ),
        ("shell-blank-style", "New Blank Style", false, None),
        ("shell-copy-note", "Copy Note", false, None),
    ] {
        item(&app.handle(), &file, id, title, enabled, key)?;
    }
    let rewrite = Submenu::new(app, "Rewrite Note As", false)?;
    file.append(&rewrite)?;
    item(
        &app.handle(),
        &file,
        "shell-recover",
        "Recover Interrupted Recording…",
        false,
        None,
    )?;
    item(
        &app.handle(),
        &file,
        "shell-delete-all",
        "Delete All Sessions…",
        false,
        None,
    )?;
    item(
        &app.handle(),
        &file,
        "shell-close",
        "Close Window",
        true,
        Some("CmdOrCtrl+W"),
    )?;
    menu.append(&file)?;

    let edit = Submenu::new(app, "Edit", true)?;
    for p in [
        P::undo(app, None)?,
        P::redo(app, None)?,
        P::cut(app, None)?,
        P::copy(app, None)?,
        P::paste(app, None)?,
        P::select_all(app, None)?,
    ] {
        edit.append(&p)?;
    }
    item(
        &app.handle(),
        &edit,
        "shell-delete-session",
        "Delete Session",
        false,
        None,
    )?;
    item(
        &app.handle(),
        &edit,
        "shell-find",
        "Find",
        false,
        Some("CmdOrCtrl+F"),
    )?;
    for (id, title) in [
        ("shell-autofill", "AutoFill"),
        ("shell-dictation", "Start Dictation"),
        ("shell-emoji", "Emoji & Symbols"),
    ] {
        item(&app.handle(), &edit, id, title, false, None)?;
    }
    menu.append(&edit)?;

    let view = Submenu::new(app, "View", true)?;
    for (id, title, enabled) in [
        ("shell-note", "Note", false),
        ("shell-transcript", "Transcript", false),
        ("shell-audio", "Audio", false),
        ("shell-live-transcript", "Show Live Transcript", false),
        ("shell-manage-styles", "Note Styles", false),
        ("shell-previous", "Previous Session", false),
        ("shell-next", "Next Session", false),
        ("shell-sidebar", "Show/Hide Sidebar", false),
    ] {
        item(&app.handle(), &view, id, title, enabled, None)?;
    }
    view.append(&P::fullscreen(app, None)?)?;
    menu.append(&view)?;

    let window = Submenu::new(app, "Window", true)?;
    window.append(&P::minimize(app, None)?)?;
    window.append(&P::maximize(app, Some("Zoom"))?)?;
    item(
        &app.handle(),
        &window,
        "shell-main",
        "Scribular",
        true,
        None,
    )?;
    item(
        &app.handle(),
        &window,
        "shell-front",
        "Bring All to Front",
        true,
        None,
    )?;
    menu.append(&window)?;

    let help = Submenu::new(app, "Help", true)?;
    item(
        &app.handle(),
        &help,
        "shell-help-search",
        "Search",
        false,
        None,
    )?;
    item(
        &app.handle(),
        &help,
        "shell-help",
        "Scribular Help",
        false,
        None,
    )?;
    item(
        &app.handle(),
        &help,
        "shell-shortcuts",
        "Keyboard Shortcuts",
        true,
        None,
    )?;
    item(
        &app.handle(),
        &help,
        "shell-welcome",
        "Welcome to Scribular",
        true,
        None,
    )?;
    item(
        &app.handle(),
        &help,
        "shell-coach",
        "Make a Note Style from an Example…",
        false,
        None,
    )?;
    item(
        &app.handle(),
        &help,
        "shell-restore-sample",
        "Restore Sample Session",
        true,
        None,
    )?;
    item(
        &app.handle(),
        &help,
        "shell-support",
        "Contact Support…",
        true,
        None,
    )?;
    item(
        &app.handle(),
        &help,
        "shell-privacy",
        "Privacy at Scribular",
        true,
        None,
    )?;
    item(
        &app.handle(),
        &help,
        "shell-site",
        "scribular.com",
        true,
        None,
    )?;
    menu.append(&help)?;
    Ok(menu)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recording_labels_and_availability_follow_owner_state() {
        assert_eq!(
            recording_menu_state(false, false, false, true),
            (("Start Recording", true), ("Pause Recording", false))
        );
        assert_eq!(
            recording_menu_state(true, false, false, true),
            (("Stop Recording", true), ("Pause Recording", true))
        );
        assert_eq!(
            recording_menu_state(true, true, false, true),
            (("Stop Recording", true), ("Resume Recording", true))
        );
        assert_eq!(
            recording_menu_state(true, true, false, false),
            (("Stop Recording", true), ("Resume Recording", true))
        );
        assert_eq!(
            recording_menu_state(false, false, true, true),
            (("Start Recording", false), ("Pause Recording", false))
        );
        assert_eq!(
            recording_menu_state(false, false, false, false),
            (("Start Recording", false), ("Pause Recording", false))
        );
        assert_eq!(binding("shell-record"), Some("CmdOrCtrl+R"));
        assert_eq!(binding("shell-pause"), Some("Alt+CmdOrCtrl+R"));
        assert_eq!(note_menu_state(false, false, false, false), (false, false));
        assert_eq!(note_menu_state(true, true, false, false), (true, true));
        assert_eq!(note_menu_state(true, true, true, false), (false, false));
    }
}
