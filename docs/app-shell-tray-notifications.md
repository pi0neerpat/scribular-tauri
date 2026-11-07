# App shell: tray, close-to-hide, quit confirm, notifications

Shipped in PR #18. Spec: `plans/parity/08-app-shell-tray-notifications.md`. macOS only.

## What it does

- **Tray (menu bar).** Tooltip "Scribular". Menu: Show Scribular, Start Recording or Stop Recording, Quit. The icon is `tray-idle.png` when idle and `tray-recording.png` while recording. With multi-speaker on, Stop becomes a submenu: "2 speakers", "3 speakers", "4+ speakers", "Skip speaker detection".
- **Close-to-hide.** The red close button does not quit. The window is hidden and recording continues. The tray's Show item, or a Dock click, brings it back. If the tray failed to install, closing exits the app.
- **Cmd+Q / Quit during recording.** If a recording is active, quitting is blocked, the window is shown and focused, and a confirm dialog (`QuitConfirm.svelte`) opens. Confirming stops the recorder and exits.
- **Summary notification.** When a summary reaches Done, a native "Summary ready" notification is sent if the setting is on, the window is not visible and focused, and that session has not already been notified.
- **Tray failures.** A failed tray Start (renderer side) or tray Stop shows a native notification with the error. A tray Start failure also shows the window.
- **External links.** Clicks on `http(s)` links to another origin, and on `mailto:` links, open in the system handler. `open_external_link` rejects every other scheme.
- **Navigation lock.** History and Settings buttons are disabled while any model is `downloading` or `verifying`.
- **Background animation.** A canvas starfield. It animates only if the setting is on, the document is visible, and reduced-motion is off. It speeds up while recording.

## How it works

- `src-tauri/src/shell/tray.rs`: `tray_model` (pure menu description), `install`, `build_menu`, `refresh(app, recording)` (rebuilds menu and icon), `stop` (calls `Recorder::stop_with_speakers`, logs, emits `tray-stop-failed`, notifies), `show_window`. `src-tauri/src/shell/mod.rs` only exports `tray`.
- `src-tauri/src/lib.rs`:
  - The `on_window_event` handler turns `CloseRequested` into `prevent_close` plus a `window-close-requested` event.
  - `RunEvent::ExitRequested` checks `Recorder::quit_confirmation_needed()` and the `QuitConfirmed` flag, and emits `quit-requested`.
  - `RunEvent::Reopen` shows the window.
  - `setup` installs the tray and the default app menu (so Cmd+C/V/Q work). A tray install error is only logged as a WARN.
  - `tauri_plugin_notification` is registered here.
- `src-tauri/src/commands.rs`:
  - `quit_app`, `confirm_quit`.
  - `notify_tray_start_failure`, `request_notification_permission`, `open_external_link`.
  - `TauriJobEvents::progress` refreshes the tray on every progress event and sends the summary notification. `should_notify_summary` and `SummaryNotices` do the gating and de-duplication.
- Frontend:
  - `src/App.svelte` listens for `window-close-requested` (if Settings has unsaved changes it asks first, then calls `getCurrentWindow().hide()`), `quit-requested`, `tray-start-recording` and `tray-stop-failed`.
  - `src/lib/api.ts` wraps the commands and events.
  - `src/main.ts` holds the global link-click handler.
  - `src/lib/navigation.ts` holds `navigationLockedBy`.
  - `src/lib/components/BackgroundAnimation.svelte` and `background-animation.ts` (`shouldAnimate`) run the starfield.
  - `src/lib/components/Settings.svelte` has the two toggles.
- Window: `src-tauri/tauri.conf.json` sets `titleBarStyle: Overlay` and a 670x552 minimum.

## Config and data

- `AppSettings` (`src/lib/types/AppSettings.ts`, persisted by the Rust store): `notifySummaryComplete` (default false in the frontend store) and `showBackgroundAnimation` (default true).
- Turning on "Show summarization complete alert" calls `request_notification_permission`. If macOS denies it, the toggle reverts to off.
- `src-tauri/capabilities/default.json` (window `main`) grants `core:event:default`, `core:window:allow-destroy`, `core:window:allow-hide`, `core:window:allow-show` and `core:window:allow-start-dragging`. Without `allow-hide` the close-to-hide call from the frontend fails.
- Crate: `tauri-plugin-notification = "=2.5.0"`. Tray icons are `src-tauri/icons/tray-idle.png` and `tray-recording.png`, embedded with `include_bytes!`.

## How to test

```
npm run check
npm test
cargo test --manifest-path src-tauri/Cargo.toml
```

- Rust unit tests: `tray_model_matches_menu_states`, `tray_icons_have_visible_pixels` (`tray.rs`), `should_notify_summary` and the `open_external_link` tests (`commands.rs`).
- Frontend tests: `src/lib/navigation.test.ts`, `src/lib/components/background-animation.test.ts`.
- UI flow: build and run `uitest` as described in `tools/uitest/README.md` and the project `CLAUDE.md`. For example, `as13-dirty-close-cancel.uitest` in `tools/uitest/scenarios/` exercises the close path. Start recording tests from `system-audio-smoke.uitest`.

## Known limits

- The `uitest` runner cannot reach the tray menu or notification banners. Those paths have no automated UI coverage.
- 13 manual checks from the spec are unverified.
- Long-capture runs with the fake engine showed timing failures.
- Notifications need the macOS notification permission. If it is denied, the "Summary ready" notification and the tray error notifications do not appear.
- The summary notification is sent from Rust, not the renderer. It fires only on the Done transition, once per session.
