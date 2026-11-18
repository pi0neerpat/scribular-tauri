# App shell: menus, Settings, license gate, updates, Delete All

Shipped in PRs #26-#30. Spec: `plans/2026-11-16-app-shell-spec.md` (R1-R16, AS1-AS19). macOS only. Tray and close-to-hide behaviour from PR #18 is described in `docs/app-shell-tray-notifications.md`; this page covers what the app-shell work added or changed.

## What it does

- **Main window and sidebar.** One main window with a searchable session sidebar: search, New Recording, Note Styles, then sessions grouped Today, Yesterday, Earlier (local calendar). Search is a case-insensitive substring match on title, date, transcript, or note (`search_sessions`). A **Delete All...** button sits below the session list.
- **Menu bar.** One native menu bar: Scribular, File, Edit, View, Window, Help. Item enablement follows recording, selection, note, model-lock, and license state, and each command rechecks state when it runs, so a stale menu cannot start forbidden work. App items route to the main window.
- **Settings window.** Scribular > Settings... opens one separate window (a second call focuses it). A newly opened window starts on General. Panes in order: General (note-ready alert, anonymous usage/crash data, automatic updates, version), Appearance, Recording, Keyboard Shortcuts, License. Help > Keyboard Shortcuts opens the Keyboard Shortcuts pane.
- **Automatic updates preference.** General > automatic updates. Off stops scheduled checks, including after restart. Scribular > Check for Updates... still works. Turning it back on starts a check at once. A failed save leaves the switch at the last saved value and shows an error.
- **Read-only gate.** When the trial ends or the license is invalid or lapsed, the record screen, each note, and Note Styles show a renewal banner (Enter License..., Buy). Existing sessions, notes, transcripts, and audio stay viewable and copyable. New content work is blocked in the UI, in the menu, and in Rust. A recording or job that already started can finish or be cancelled. Deleting is never gated: Delete Session, Delete All Sessions, and deleting a custom style work in every license state.
- **Update-ready alert.** When a downloaded update is ready, a native alert says "An update is ready." with the version and two buttons, Restart Now and OK. It shows once per version and waits until recording stops. OK leaves the update to install on the next normal quit. Restart Now uses the guarded quit path, so the usual quit confirmation still appears if work is running. There is no UpdateBar in the main window.
- **Delete All Sessions.** File > Delete All Sessions... and the sidebar button are disabled while recording or with no sessions. A native confirmation names the session count and the loss of audio, transcripts, and notes. Cancel is the default and Return/Escape cancel. Only the red Delete All Sessions button clears data. The sidebar refreshes afterwards, and a partial failure reports deleted and skipped counts.
- **Tray and Dock.** The tray menu has only Show Scribular, Start/Stop Recording (with the speaker submenu), and Quit. With the main window closed, only the tray icon remains and the Dock icon is hidden; it returns when a window is shown. Quit asks for confirmation only while recording or while a note or summary is being generated.

## How it works

- `src-tauri/src/shell/menu.rs`: builds the menu (IDs start `shell-`), computes enablement (`shell-delete-all` is enabled when not recording and sessions exist), and opens Settings (`open_settings`). Menu clicks reach the webview as the `shell://menu` event with the item ID. `shell://settings-pane` switches pane in an open Settings window.
- `src-tauri/src/shell/tray.rs`, `src-tauri/src/shell/mod.rs`: tray menu, window and Dock visibility, and `native_choice`, the helper that shows a native alert and returns the clicked button.
- `src-tauri/src/lib.rs`: installs menu, tray, and updater; registers commands; handles close, quit, and reopen events.
- `src-tauri/src/commands.rs`:
  - `get_app_settings` / `set_app_settings` save settings and emit `app-settings://changed`. Switching automatic updates from off to on starts a check on a background thread.
  - `show_update_ready_alert` shows the update alert. It returns nothing while recording or when that version was already claimed (`UpdateManager::claim_ready_alert`). Otherwise it returns whether Restart Now was chosen.
  - `clear_all_sessions` shows the Delete All confirmation and returns `null` if it was cancelled or not allowed.
  - `check_for_updates`, `get_update_state`, and `install_update` (Restart Now) drive updates. State changes are emitted as `update://state`.
  - License gate: mutation commands call `licensing.require_access()?` and return a stable `accessDenied` error (JSON, parsed by `accessDenied()` in `src/lib/api.ts`). The gated list is pinned by a test: `restore_sample_sessions`, `update_session`, `undo_summary`, `create_prompt`, `duplicate_prompt`, `update_prompt`, `set_prompt_starred`, `set_active_prompt`, `regenerate_summary`, `recover_session`, `update_speaker_map`, `start_recording`, `transcribe_file`. The same test lists commands that must stay open, such as `delete_session`, `clear_all_sessions`, `delete_prompt`, `stop_recording`, and `activate_license`.
- `src-tauri/src/license/mod.rs`: `access_status` and `require_access` are the single access decision.
- `src-tauri/src/update/mod.rs`: `UpdateManager` checks, downloads, stages, and installs on quit. `check_automatically` reads the setting before each scheduled check.
- Frontend: `src/App.svelte` (sidebar, menu routing, update alert, Delete All button), `src/lib/api.ts` (command wrappers), `src/lib/components/Settings.svelte`, `LicenseGate.svelte`, `LicenseSettings.svelte`, `UpdateAlert.svelte`, `src/lib/license.ts`, `src/lib/app-settings.ts`. Generated types are in `src/lib/types/` (`AppSettings.ts`, `AccessState.ts`).

## Config and data

- Settings are rows in the `settings` key/value table of the encrypted `scribular.db` in the app data directory (`src-tauri/src/store/settings.rs`). Keys relevant here:
  - `check_for_updates_automatically`: default on for a new install. Exposed as `checkForUpdatesAutomatically` in `AppSettings`.
  - `notify_summary_complete`, `show_background_animation`: other General/Appearance switches.
  - `pending_update_install`, `skipped_update_version`: update bookkeeping.
- Updater config is in `src-tauri/tauri.conf.json` under `plugins.updater`:
  - `endpoints`: `https://github.com/pi0neerpat/scribular-tauri-releases/releases/latest/download/latest.json`
  - `pubkey`: currently the placeholder `REPLACE_WITH_TAURI_UPDATER_PUBKEY`.
  - `bundle.createUpdaterArtifacts` is `true`, so builds need `TAURI_SIGNING_PRIVATE_KEY` (see `CLAUDE.md`).
- Settings window capability: `src-tauri/capabilities/default.json` lists the `main` and `settings` windows.

## How to test

- Rust: `cargo test --manifest-path src-tauri/Cargo.toml`. Covers the access gate (AS6, AS9), the settings round trip and update manager (AS8, AS13), the tray builder (AS17), and the command inventory test above.
- Frontend: `npm test` (vitest plus release-tool tests). Relevant files: `src/lib/license.test.ts`, `src/lib/components/license-gate.svelte.test.ts`, `license-settings.svelte.test.ts`, `update-alert.svelte.test.ts`, `update-status.test.ts`, `src/lib/api.test.ts`.
- Real app (uitest): `swift run --package-path tools/uitest uitest <scenario> --app <Scribular.app>`. Setup and build steps are in `CLAUDE.md` and `tools/uitest/README.md`. Scenarios: `as13-dirty-close-cancel.uitest`, `close-hides-dock-restore.uitest`, `tray-status-item.uitest`. Manual-only checks: AS1, AS4, AS5, AS7, AS10, AS12, AS14, AS18. AS5 denied-license menus and AS10 are tracked in issue #29.

## Known limits

- **AS14 is not verified end to end.** The case "update becomes ready during recording, alert waits for Stop, OK installs on normal quit, Restart Now relaunches" has no real-app run because no signed test update and no published `latest.json` exist. Unit tests cover the once-per-version claim, the recording deferral, and the busy-restart confirmation only.
- **Before release:** replace the placeholder `pubkey` with the real Tauri updater public key, sign release builds with the matching private key, and publish `latest.json` and the signed artifacts at the endpoint above. Until then update checks cannot succeed.
- The alert and Delete All confirmation use native macOS dialogs; other platforms return "Native alerts require macOS".
