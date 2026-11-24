use tauri::{
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle,
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
    use super::recording::{background, BackgroundAction::*};

    let idle = Image::from_bytes(include_bytes!("../../icons/tray-idle.png"))?;
    let menu = build_menu(app.handle(), false, false)?;
    TrayIconBuilder::with_id("main")
        .icon(idle)
        .icon_as_template(true)
        .tooltip("Scribular")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => {
                super::show_main(app);
            }
            "start" => background(app, Start),
            "stop" | "stop4" => background(app, Stop(None)),
            "stop2" => background(app, Stop(Some(2))),
            "stop3" => background(app, Stop(Some(3))),
            "skip" => background(app, Stop(Some(0))),
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    Ok(())
}

fn display_chord(chord: tauri_plugin_global_shortcut::Shortcut) -> String {
    use tauri_plugin_global_shortcut::Modifiers;
    let mut label = String::new();
    for (modifier, glyph) in [
        (Modifiers::CONTROL, '⌃'),
        (Modifiers::ALT, '⌥'),
        (Modifiers::SHIFT, '⇧'),
        (Modifiers::SUPER, '⌘'),
    ] {
        if chord.mods.contains(modifier) {
            label.push(glyph);
        }
    }
    let key = chord.key.to_string();
    label.push_str(
        key.strip_prefix("Key")
            .or_else(|| key.strip_prefix("Digit"))
            .unwrap_or(&key),
    );
    label
}

fn build_menu(
    app: &AppHandle,
    recording: bool,
    multi_speaker: bool,
) -> tauri::Result<Menu<tauri::Wry>> {
    let chord = crate::shortcuts::active(app)
        .and_then(|chord| chord.parse::<tauri_plugin_global_shortcut::Shortcut>().ok())
        .map(|chord| format!(" ({})", display_chord(chord)))
        .unwrap_or_default();
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
                format!("Start Recording{chord}"),
                true,
                None::<&str>,
            )?),
            TrayItem::Stop => Box::new(MenuItem::with_id(
                app,
                "stop",
                format!("Stop Recording{chord}"),
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
                    format!("Stop Recording{chord}"),
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

pub fn refresh(app: &AppHandle, recording: bool, multi_speaker: bool) {
    let Some(tray) = app.tray_by_id("main") else {
        return;
    };
    if let Ok(menu) = build_menu(app, recording, multi_speaker) {
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
        for (recording, multi_speaker, action) in [
            (false, false, TrayItem::Start),
            (false, true, TrayItem::Start),
            (true, false, TrayItem::Stop),
            (true, true, TrayItem::SpeakerSubmenu),
        ] {
            assert_eq!(
                tray_model(recording, multi_speaker),
                [
                    TrayItem::Show,
                    TrayItem::Separator,
                    action,
                    TrayItem::Separator,
                    TrayItem::Quit
                ]
            );
        }
    }

    #[test]
    fn chord_label_uses_mac_modifier_order() {
        assert_eq!(display_chord("Shift+Super+Alt+Control+KeyR".parse().unwrap()), "⌃⌥⇧⌘R");
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
