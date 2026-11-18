use tauri::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem as P, Submenu},
    AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder,
};

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
            let _ = super::hide_dock_if_no_windows(app);
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
) -> Result<(), String> {
    let menu = app.menu().ok_or("no app menu")?;
    for (id, enabled) in [
        ("shell-new-recording", writable && !locked && !recording),
        ("shell-manage-styles", !locked),
        ("shell-blank-style", writable && !locked && !recording),
        ("shell-find", !locked),
        ("shell-delete-all", !recording && has_sessions),
        ("shell-delete-session", !locked && selected && !recording),
        ("shell-copy-note", !locked && has_note),
        (
            "shell-recover",
            writable && !locked && recoverable && !recording,
        ),
        ("shell-restore-sample", writable && !locked),
    ] {
        for top in menu.items().map_err(|e| e.to_string())? {
            if let tauri::menu::MenuItemKind::Submenu(sub) = top {
                if let Some(tauri::menu::MenuItemKind::MenuItem(item)) = sub.get(id) {
                    item.set_enabled(enabled).map_err(|e| e.to_string())?;
                }
            }
        }
    }
    Ok(())
}

fn item(
    app: &AppHandle,
    menu: &Submenu<tauri::Wry>,
    id: &str,
    title: &str,
    enabled: bool,
    shortcut: Option<&str>,
) -> tauri::Result<()> {
    menu.append(&MenuItem::with_id(app, id, title, enabled, shortcut)?)
}

pub fn build(app: &tauri::App) -> tauri::Result<Menu<tauri::Wry>> {
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
