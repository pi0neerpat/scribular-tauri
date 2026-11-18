pub mod menu;
pub mod tray;

use tauri::{AppHandle, Manager, WebviewWindow};

pub fn show_main(app: &AppHandle) -> Option<WebviewWindow> {
    let window = app.get_webview_window("main")?;
    show_window(&window).ok()?;
    let _ = window.set_focus();
    Some(window)
}

pub fn show_window(window: &WebviewWindow) -> tauri::Result<()> {
    restore_dock_icon(window.app_handle());
    window.show()
}

fn restore_dock_icon(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    if app
        .set_activation_policy(tauri::ActivationPolicy::Regular)
        .is_err()
    {
        crate::diag::log::log("WARN", "shell", "could not restore the Dock icon");
    }
}

pub fn hide_dock_if_no_windows(app: &AppHandle) -> tauri::Result<()> {
    #[cfg(target_os = "macos")]
    if app.tray_by_id("main").is_some()
        && app.webview_windows().values().all(|window| {
            !window.is_visible().unwrap_or(true) && !window.is_minimized().unwrap_or(true)
        })
    {
        app.set_activation_policy(tauri::ActivationPolicy::Accessory)?;
    }
    Ok(())
}

/// Shows a modal alert. `Some(false)` = first button (Return/Escape default),
/// `Some(true)` = second button, `None` = `detail` returned `None` (alert skipped)
/// or AppKit returned an unrecognized response.
#[cfg(target_os = "macos")]
pub async fn native_choice(
    app: tauri::AppHandle,
    title: &'static str,
    detail: impl FnOnce() -> Result<Option<String>, String> + Send + 'static,
    buttons: [&'static str; 2],
    destructive: bool,
) -> Result<Option<bool>, String> {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{
        NSAlert, NSAlertFirstButtonReturn, NSAlertSecondButtonReturn, NSAlertStyle,
    };
    use objc2_foundation::NSString;
    use tauri::async_runtime::{channel, Mutex};

    static ALERT: Mutex<()> = Mutex::const_new(());
    let _alert = ALERT.lock().await;
    let (send, mut receive) = channel(1);
    app.run_on_main_thread(move || {
        let answer = (|| {
            let mtm = MainThreadMarker::new().ok_or("Alert requires the main thread")?;
            let Some(detail) = detail()? else {
                return Ok(None);
            };
            let alert = NSAlert::new(mtm);
            alert.setMessageText(&NSString::from_str(title));
            alert.setInformativeText(&NSString::from_str(&detail));
            // AppKit makes the first button the Return default and maps Cancel to Escape.
            alert.addButtonWithTitle(&NSString::from_str(buttons[0]));
            let action = alert.addButtonWithTitle(&NSString::from_str(buttons[1]));
            if destructive {
                alert.setAlertStyle(NSAlertStyle::Critical);
                action.setHasDestructiveAction(true);
            }
            let response = alert.runModal();
            Ok(if response == NSAlertFirstButtonReturn {
                Some(false)
            } else if response == NSAlertSecondButtonReturn {
                Some(true)
            } else {
                None
            })
        })();
        let _ = send.try_send(answer);
    })
    .map_err(|e| e.to_string())?;
    receive.recv().await.ok_or("Native alert did not respond")?
}
