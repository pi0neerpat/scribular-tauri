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
                stop(app, Some(4));
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
        let _ = tauri::async_runtime::spawn_blocking(move || {
            let _ = recorder.stop_with_speakers(speakers);
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
    let show = MenuItem::with_id(app, "show", "Show Scribular", true, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    items.push(Box::new(show));
    items.push(Box::new(sep1));
    if recording && multi_speaker {
        let header = MenuItem::with_id(
            app,
            "header",
            "⏹ Stop - How many speakers?",
            false,
            None::<&str>,
        )?;
        let sep = PredefinedMenuItem::separator(app)?;
        let mut choices: Vec<Box<dyn tauri::menu::IsMenuItem<tauri::Wry>>> =
            vec![Box::new(header), Box::new(sep)];
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
        let choice_refs: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> =
            choices.iter().map(|i| i.as_ref()).collect();
        items.push(Box::new(tauri::menu::Submenu::with_id_and_items(
            app,
            "stop-submenu",
            "Stop Recording",
            true,
            &choice_refs,
        )?));
    } else {
        items.push(Box::new(MenuItem::with_id(
            app,
            if recording { "stop" } else { "start" },
            if recording {
                "Stop Recording"
            } else {
                "Start Recording"
            },
            true,
            None::<&str>,
        )?));
    }
    items.push(Box::new(PredefinedMenuItem::separator(app)?));
    items.push(Box::new(MenuItem::with_id(
        app,
        "quit",
        "Quit",
        true,
        None::<&str>,
    )?));
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
}
