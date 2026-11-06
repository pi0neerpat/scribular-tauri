use tauri::{
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayItem {
    Show,
    Separator,
    Start,
    Stop,
    SpeakerSubmenu,
    Quit,
}

pub fn tray_model(recording: bool, multi_speaker: bool) -> Vec<TrayItem> {
    let mut items = vec![TrayItem::Show, TrayItem::Separator];
    if !recording {
        items.push(TrayItem::Start);
    } else if multi_speaker {
        items.push(TrayItem::SpeakerSubmenu);
    } else {
        items.push(TrayItem::Stop);
    }
    items.extend([TrayItem::Separator, TrayItem::Quit]);
    items
}

pub fn install(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let idle = Image::from_bytes(include_bytes!("../../icons/tray-idle.png"))?;
    let menu = build_menu(app.handle(), false, false)?;
    TrayIconBuilder::with_id("main")
        .icon(idle)
        .icon_as_template(true)
        .tooltip("Scribular")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => show_window(app),
            "start" => {
                let _ = app.emit(crate::commands::TRAY_START_RECORDING_EVENT, ());
            }
            "stop" => {
                stop(app, None);
            }
            "stop2" => {
                stop(app, Some(2));
            }
            "stop3" => {
                stop(app, Some(3));
            }
            "stop4" => {
                stop(app, None);
            }
            "skip" => {
                stop(app, Some(0));
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    Ok(())
}

fn stop(app: &AppHandle, speakers: Option<u8>) {
    if let Some(recorder) = app.try_state::<std::sync::Arc<crate::jobs::Recorder>>() {
        let recorder = std::sync::Arc::clone(&recorder);
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let result =
                tauri::async_runtime::spawn_blocking(move || recorder.stop_with_speakers(speakers))
                    .await;
            let error = match result {
                Ok(Ok(_)) => return,
                Ok(Err(e)) => e.to_string(),
                Err(e) => e.to_string(),
            };
            crate::diag::log::log("ERROR", "shell", "tray stop failed");
            if let Some(recorder) = app.try_state::<std::sync::Arc<crate::jobs::Recorder>>() {
                refresh(&app, recorder.is_recording());
            }
            let _ = app.emit(crate::commands::TRAY_STOP_FAILED_EVENT, error.clone());
            use tauri_plugin_notification::NotificationExt;
            let _ = app
                .notification()
                .builder()
                .title("Scribular")
                .body(error)
                .show();
        });
    }
}

fn show_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn build_menu(
    app: &AppHandle,
    recording: bool,
    multi_speaker: bool,
) -> tauri::Result<Menu<tauri::Wry>> {
    let mut items: Vec<Box<dyn tauri::menu::IsMenuItem<tauri::Wry>>> = Vec::new();
    for item in tray_model(recording, multi_speaker) {
        items.push(match item {
            TrayItem::Show => Box::new(MenuItem::with_id(
                app,
                "show",
                "Show Scribular",
                true,
                None::<&str>,
            )?),
            TrayItem::Separator => Box::new(PredefinedMenuItem::separator(app)?),
            TrayItem::Start => Box::new(MenuItem::with_id(
                app,
                "start",
                "Start Recording",
                true,
                None::<&str>,
            )?),
            TrayItem::Stop => Box::new(MenuItem::with_id(
                app,
                "stop",
                "Stop Recording",
                true,
                None::<&str>,
            )?),
            TrayItem::SpeakerSubmenu => {
                let mut choices: Vec<Box<dyn tauri::menu::IsMenuItem<tauri::Wry>>> = vec![
                    Box::new(MenuItem::with_id(
                        app,
                        "header",
                        "⏹ Stop - How many speakers?",
                        false,
                        None::<&str>,
                    )?),
                    Box::new(PredefinedMenuItem::separator(app)?),
                ];
                for (id, text) in [
                    ("stop2", "2 speakers"),
                    ("stop3", "3 speakers"),
                    ("stop4", "4+ speakers"),
                    ("skip", "Skip speaker detection"),
                ] {
                    choices.push(Box::new(MenuItem::with_id(
                        app,
                        id,
                        text,
                        true,
                        None::<&str>,
                    )?));
                }
                let refs: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> =
                    choices.iter().map(|i| i.as_ref()).collect();
                Box::new(tauri::menu::Submenu::with_id_and_items(
                    app,
                    "stop-submenu",
                    "Stop Recording",
                    true,
                    &refs,
                )?)
            }
            TrayItem::Quit => Box::new(MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?),
        });
    }
    let refs: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> =
        items.iter().map(|i| i.as_ref()).collect();
    Menu::with_items(app, &refs)
}

pub fn refresh(app: &AppHandle, recording: bool) {
    let multi = app
        .try_state::<std::sync::Arc<crate::store::Store>>()
        .and_then(|s| s.recording_settings().ok())
        .is_some_and(|s| s.multi_speaker);
    let Some(tray) = app.tray_by_id("main") else {
        return;
    };
    if let Ok(menu) = build_menu(app, recording, multi) {
        let _ = tray.set_menu(Some(menu));
    }
    let (bytes, template): (&[u8], bool) = if recording {
        (include_bytes!("../../icons/tray-recording.png"), false)
    } else {
        (include_bytes!("../../icons/tray-idle.png"), true)
    };
    if let Ok(icon) = Image::from_bytes(bytes) {
        let _ = tray.set_icon(Some(icon));
        let _ = tray.set_icon_as_template(template);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tray_model_matches_menu_states() {
        assert_eq!(
            tray_model(false, false),
            [
                TrayItem::Show,
                TrayItem::Separator,
                TrayItem::Start,
                TrayItem::Separator,
                TrayItem::Quit
            ]
        );
        assert_eq!(tray_model(true, false)[2], TrayItem::Stop);
        assert_eq!(tray_model(true, true)[2], TrayItem::SpeakerSubmenu);
    }

    #[test]
    fn tray_icons_have_visible_pixels() {
        // A fully transparent template icon installs fine but draws nothing in the menu bar.
        for bytes in [
            include_bytes!("../../icons/tray-idle.png").as_slice(),
            include_bytes!("../../icons/tray-recording.png").as_slice(),
        ] {
            let icon = Image::from_bytes(bytes).unwrap();
            assert!(icon.rgba().chunks(4).any(|px| px[3] > 0));
        }
    }
}
