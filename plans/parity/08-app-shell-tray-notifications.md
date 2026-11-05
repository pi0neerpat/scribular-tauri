# App shell: tray, notifications, window, quit — Tauri parity spec
Status: draft for cwf; Date: 2026-11-03


> **Dependency policy (Patrick, 2026-11-05; supersedes every dependency-review, Patrick-approval, publish-date cutoff, two-week rule, exact-pin, OSV/RustSec, owner-check or `build.rs`-review requirement in this spec and in `plans/DEPENDENCY_REVIEW_*.md`).** Before adding a crate or npm package, check only that (1) it is commonly used (substantial downloads or dependents) and (2) it has had a release or commit within the last 12 months. Record the result in one or two lines in the PR description. No `DEPENDENCY_REVIEW_*.md` doc and no approval step. Prefer dependencies already in the tree. This covers packages only: a pinned model file still needs its URL, size and sha256 taken from the upstream file, never invented.

Electron refs are in `/Volumes/My Shared Files/Scribular/notes-assistant-electron` (v1.5.10), written as `src/...:line`. Tauri refs are in `scribular-tauri`.

## Intent (why users need it; who)
Scribular is a menu-bar-friendly recorder. Therapists and clinicians start a recording, switch to other work or hide the window, and expect to:
- start and stop from the menu bar without opening the window (the tray was added for exactly this, `plans/TRAY_SPEAKER_SUBMENU.md`);
- be told when the summary is ready, if they opted in;
- have a window that looks like a Mac app (hidden title bar, drag region, minimum size);
- not lose a recording by closing a window or hitting Cmd+Q.

The users are new Tauri users on macOS 14+ (Apple Silicon). Nothing is migrated from Electron.

## Electron behaviour today (with file:line refs)

### Tray / menu bar
- `createTray` runs once at app ready (`src/index.js:536-538`) and is destroyed in `before-quit` (`src/index.js:560-562`).
- The icon is white when idle and red when recording, falling back to `icon.png` and then `icon.icns` (`src/main/tray.js:13-29`). It is resized to 22px if wider than 64px (`tray.js:145-152,164-167`). The tooltip is "Scribular" (`tray.js:170`).
- Clicking the tray icon does nothing on purpose; the user must pick "Show Scribular" (`tray.js:173-176`).
- Menu: "Show Scribular", separator, the record item, separator, "Quit" (`tray.js:110-116`).
- Record item when idle: "Start Recording" with a play icon (`tray.js:93-108`). Recording: "Stop Recording" with a stop icon.
- Menu icons use macOS template images `icon-playTemplate.png` and `icon-stopTemplate.png` (`tray.js:36-41`).
- Multi-speaker stop submenu: while recording with `config.multiSpeaker.enabled`, the stop item becomes a disabled header "⏹ Stop - How many speakers?" plus "2 speakers", "3 speakers", "4+ speakers" (`numSpeakers: null`) and "Skip speaker detection" (`skipDiarization: true`) (`tray.js:50-92`). This is what spec 02 owns. This spec provides the menu hook.
- State flow, with the renderer as the source of truth:
  - the renderer sends `RECORDING_STATE_CHANGED(bool)` on recording start, on stop, and on every status event (`src/renderer/views/recording.js:362,446,593-595`; preload `src/preload/preload.js:98-99`);
  - main rebuilds the menu and icon (`tray.js:133-153,179-181`), re-reading `multiSpeaker.enabled` each time.
- Tray Start/Stop is relayed to the renderer as `TRAY_START_RECORDING` / `TRAY_STOP_RECORDING(options)` (`tray.js:61-103`). The renderer handlers are in `recording.js:567-581`:
  - Start: no-op if the licence is restricted (`isRestricted()`), otherwise show the recording view and click the Start button if it is enabled. It does not bring the window to the front.
  - Stop: `finishRecording(options ?? null)` if recording.
- If the window has been destroyed, `getMainWindow()` returns a dead window and the tray items silently do nothing (`tray.js:59-61,122-127`; see Quit).
- If a tray Start fails (permission, model missing), the error appears only in the hidden window. **Electron gap.**

### Native notifications
- `APP_SHOW_NOTIFICATION` (`src/main/registerIpcHandlers.js:305-313`) is not a native notification. It is a modal `dialog.showMessageBox({type:"info", title:"Scribular", message, buttons:["OK"]})`. It also:
  - logs the message;
  - reports it to the error reporter if it matches `/error|failed|invalid|please try again/i`;
  - is skipped under `--e2e-test`.
- The renderer uses it for every alert and validation message (`settings.js:333,432,804,939-1097`, many call sites), as well as for "Summarization complete." (`session.js:1759-1763`).
- Setting `showSummarizationCompleteAlert`: the default is `false` (`src/store/defaults.js:81`) and it is persisted through `store.js:101,123-124`. The Settings toggle is `showSummarizationAlertToggle` (`settings.js:768,893`).
- The summary-complete trigger fires in the renderer's summary-stream-complete handler (`session.js:1758-1763`). The code checks `config.showSummarizationCompleteAlert !== false`.
  - Because the stored value is a boolean and the default is false, it is effectively "on only if enabled".
  - It fires on rewrite/regenerate too. Whether it fires if the user navigated away before completion is unclear; it is driven by the renderer, not main.

### Window
- Creation (`src/index.js:215-239`):
  - 670x552 (1200 wide in debug), min 670x552, resizable;
  - `titleBarStyle:"hidden"`, `frame:false`, `transparent:true`, `vibrancy:"under-window"`, `autoHideMenuBar:true`;
  - the window is shown on `ready-to-show` to avoid a white flash (`index.js:252-264`);
  - traffic lights are positioned at x:9, y:8 (`index.js:241-245`).
- Title bar CSS (`src/renderer/ui/titlebar.css`): the bar is 30px high (`:7-8`), the whole bar is `-webkit-app-region: drag` (`:16`), and the buttons inside it are `no-drag` (`:31,45,67,154`).
- Custom min/close buttons and edge resize handles exist only for non-mac (Windows):
  - `client.js:123-144` shows the controls only if `!isMac`;
  - `client.js:146-219` is the manual resize via `WINDOW_RESIZE` / `WINDOW_GET_BOUNDS`;
  - main handlers are at `registerIpcHandlers.js:80-93,394-400`.
  On macOS the native traffic lights are used, so **these four IPC items have no macOS purpose** and are out of scope (Windows is out).
- Splash screen: **not shipped.**
  - `docs/splash.html` is an empty `.splash-container` (13 lines) that links `splash.css`.
  - The only reference is commented-out code (`src/index.js:338-341`).
  - Nothing to port.
- View navigation (`src/renderer/ui/viewNavigation.js`):
  - Views: `recording | settings | session-detail` (`:8-12`). Switching hides all, swaps the sidebar (settings sidebar vs sessions sidebar), shows the target, and toggles the Record button's active class (`:114-165`).
  - Navigation is locked while the local AI model downloads (`isDownloading`, `:21-23,116-118`). The titlebar Settings button, back buttons, sidebar record button and session list items are disabled and greyed (`:59-108`).
  - Navigation is NOT locked while a session is summarizing (`:26-32`).
  - Settings are lazy-loaded on entry (`:146`).
- The Electron menu is the default app menu (`autoHideMenuBar` only affects Windows/Linux). This is relied on implicitly for Cmd+C/V/Q/W/M.

### Background animation
- `particles.js` is a three.js starfield: 15,000 points, WebGL, `PerspectiveCamera(60)` (`src/renderer/features/particles.js:1-14,43-60`).
  - It idles at speed 0 and accelerates to 0.8 while recording (`startAnimation` / `stopAnimation`, `:145-151`, called at `recording.js:341,376,386,429,665`).
  - The renderer resizes with the window (`:100`).
- Setting `showBackgroundAnimation` defaults to `true` (`defaults.js:80`). A Settings toggle (`settings.js:57-70,767,892`) changes it. When it is off, the `#animation-background` element is `display:none` and the rAF frame is cancelled (`particles.js:20-26,158-164`; `client.js:82`).
- Likely bug: when the setting is turned back on, `refreshAnimationState` only unhides the element. If already initialised it never restarts `animate()` after the earlier `cancelAnimationFrame` (`:165-173`). Not confirmed by running it; `init()` sits outside the lines read.
- The loop runs every frame even at speed 0 and with the window hidden. No visibility pause was found. Cost: constant GPU work.

### Recording-limit reminder
- todo.md:15 lists "If a session goes beyond 55 minutes, show a native notification dialog popup: 'Do you want to stop recording?' No, Yes. Hint this can be changed in settings. Allow turning this feature off, or changing the time limit." It is unchecked.
- A grep of `src/` for 3300/`55 * 60`/"Do you want to stop"/max-recording identifiers found nothing. **Electron does not ship it, so it is a candidate, not parity.** The reason for 55 minutes is not stated (possibly a model or transcription ceiling). Unclear.

### External links
- `OPEN_EXTERNAL_LINK` (`registerIpcHandlers.js:250-260`) parses the URL and calls `shell.openExternal` only for `http:`, `https:` and `mailto:`. Invalid URLs and every other scheme are silently ignored.
- Callers: `settings.js:1031` (mailto support), `settings.js:1038` (pricing), `client.js:440` (release notes).
- No `will-navigate` or `setWindowOpenHandler` guard exists in `src/` (grep found none). An anchor with `target=_blank` (`index.html:731`) is only handled by the JS click handler.
- The privacy-pane deep links (`x-apple.systempreferences:`) go through a different path (`src/service/audio/audio.js:690-698`). Tauri already has `open_privacy_settings` (`src-tauri/src/commands.rs:226`).

### Quit behaviour
- Closing the window: `WINDOW_CLOSE` calls `w.close()`, which destroys the window. On macOS the red button also destroys it.
- `window-all-closed` (`index.js:592-599`) then:
  - calls `stopOllama()`;
  - calls `cleanup()`, which terminates the audio worker and clears recording state (`src/service/audio/audio.js:671-688`);
  - does not quit on macOS.
- Result today: **closing the window mid-recording kills the audio worker.** Whether partial audio or transcript is saved is not visible in the code read; treat it as lost or unclear.
- The tray stays, but "Show Scribular" does nothing because the window is destroyed (`tray.js:122-127`). Only the Dock click restores a window (`app.on("activate")`, `index.js:601-608`), and it creates a fresh window.
- Cmd+Q / tray Quit: `before-quit` destroys the tray, clears intervals, fires analytics (spec 06) and calls `cleanup()` (`index.js:560-587`). There is no confirmation, even mid-recording or mid-summary.
- No `powerSaveBlocker` is used (grep found none).

## Tauri state today (what exists, what is missing, with file refs)
- Window: a single default window, 900x640, with native decorations (`src-tauri/tauri.conf.json:12-17`). There is no min size, no hidden title bar, no drag region, no vibrancy, and no `on_window_event` / `RunEvent` handling (`src-tauri/src/lib.rs:47-115`: `.run()` with no exit hooks).
- Capabilities: `core:event:default` only (`src-tauri/capabilities/default.json`). The main window label is `main`.
- Tray, notifications, opener and dialog are all absent. The `tauri` crate has `features = []` (`Cargo.toml`), and no tauri plugins are listed.
- Recording state is owned by Rust:
  - `Recorder::current()` (`src-tauri/src/jobs/session.rs:221-225`) and the `current_recording` command (`commands.rs:240`);
  - session status events are emitted through `JobEvents` (`jobs/mod.rs:50-51`), including `SessionStatus::Recording` on start (`session.rs:~270`) and `Done` in the state machine (`jobs/state.rs:13-16`).
  - So the Electron renderer→main `RECORDING_STATE_CHANGED` round trip is unnecessary; the tray subscribes inside Rust.
- Quit: `impl Drop for Recorder` (`session.rs:370-385`) abandons the recording and marks jobs shutdown, so the next launch marks the session interrupted and keeps audio (KTD2 in the plan). **Not verified:** Tauri 2 `run()` ends in a process exit, and managed state may not be dropped on Cmd+Q. An explicit `RunEvent::ExitRequested` / `Exit` hook that calls the same shutdown is needed.
- Settings: a SQLite key/value `settings` table with `get_setting` / `set_setting` (`store/settings.rs:36-48`), and a typed `RecordingSettings` (`:14-22`) exposed by `get/set_recording_settings` (`commands.rs:167-180`). There are no app-shell settings.
- Frontend: `src/App.svelte` has only `record | settings` views (`:13`, `:38-43`), a heartbeat listener and no animation. `src/lib/api.ts` holds the wrappers. A cargo test (`lib.rs` tests, `every_command_has_an_api_wrapper`, `every_event_has_an_api_listener`) enforces command/event ↔ wrapper parity, so every new command or event needs a wrapper.
- The earlier plan explicitly cut "tray menu, custom window chrome" (`plans/2026-10-19-2001-...-plan.md:131`, `plans/TAURI_MVP.md:46`). This spec brings them back.

## Goals / Non-goals / Anti-goals
Goals
- Menu-bar Start/Stop with state-reflecting icon and a "Show Scribular" item.
- Opt-in native "summary ready" notification.
- A Mac-style window: overlay title bar, drag region, min size.
- A window close that never loses a recording.
- Safe external-link opening.
- Background animation with an on/off setting.
- Navigation locked while a model downloads.

Non-goals
- Windows/Linux chrome, custom min/close buttons, resize handles, `WINDOW_RESIZE` / `WINDOW_GET_BOUNDS`.
- A splash screen (Electron does not ship one).
- Window-state persistence (Electron has none).
- Migration of the Electron `showBackgroundAnimation` / `showSummarizationCompleteAlert` values.

Anti-goals
- Don't add `tauri-plugin-opener`, `three`, or `tauri-plugin-dialog` if the in-house options below suffice.
- Don't route every UI message through a native notification. Electron's modal for validation messages is a poor pattern; use in-app UI. Native notifications are for background events only.
- Don't let the window-close path touch recording state.

## Requirements (numbered R1..)
Priorities: P1 users depend on it, P2 expected, P3 cosmetic/low.

- **R0 (P1, first step).** Check each candidate below against the dependency policy at the top of this spec (commonly used; release within the last 12 months) and note the result in the PR description. No review doc, no approval step.

Quit and close (P1)
- **R1.** Closing the window (red button, Cmd+W) hides it and does not exit the app. This applies whenever the tray exists. Recording, transcription and summarization continue. Handle `WindowEvent::CloseRequested` with `prevent_close()` and `hide()`.
- **R2.** The app reopens its window from the Dock icon (`RunEvent::Reopen`, macOS), from the tray "Show Scribular", and on a second launch.
- **R3.** Cmd+Q or tray "Quit" when idle quits immediately. When a recording is active, or a session is Finalizing/Summarizing, quit is intercepted in `RunEvent::ExitRequested` (`api.prevent_exit()`). The window is shown and a frontend confirm modal ("A recording is in progress. Quit and keep what's recorded so far?" / Cancel) appears.
  - Confirm calls a `confirm_quit` command that runs the same shutdown as `Drop for Recorder` and exits.
  - The audio recorded so far is preserved and the session is interrupted on next launch (KTD2). Recovery UI is spec 09.
- **R4.** Whatever path quits (including a forced `Exit` event), the recorder shutdown path runs once. Put it in an explicit `shutdown()` called from `RunEvent::Exit`, in addition to `Drop`, and make it idempotent. Verify with the fake-engines test below.

Tray (P1)
- **R5.** Create the tray at setup using Tauri's built-in `tray-icon` feature and `tauri::menu` (no new crate beyond what that feature pulls in). It has a tooltip "Scribular". Left-click does not toggle the window; menus open on click (`show_menu_on_left_click(true)`), matching Electron's no-op click.
- **R6.** Menu: "Show Scribular", separator, the record item, separator, "Quit". The record item is "Start Recording" when idle and "Stop Recording" when `Recorder::current().is_some()`.
- **R7.** The icon swaps by state: a monochrome template icon (`icon_as_template(true)`) when idle, and a non-template red icon when recording. Assets are derived from `build/icon-taskbar-*.png` and `icon-{play,stop}Template.png` in the Electron repo (`tray.js:20-28,36-41`). Template icons cannot be tinted, which is why the red one is non-template. Menu-item icons are optional (P3).
- **R8.** The tray is driven from Rust state. A pure function `tray_model(recording: bool, multi_speaker: bool) -> Vec<TrayItem>` builds the menu, and a thin adapter turns it into a `tauri::menu::Menu`. It is rebuilt whenever a `SessionStatus` event moves a session into or out of `Recording` (including watchdog failure, `session.rs` watch thread), so the tray never shows a stale "Stop".
- **R9.** Tray Start emits a `tray-start-recording` event to the frontend. The frontend runs the same path as the Record button (settings, permission and model-ready checks). It does not use a second Rust start path, so behaviour matches the button. The window stays hidden (as in Electron). The licence-restriction check will come from spec 04.
  - **Improvement over Electron:** if the start fails (permission denied, model not ready), show the window and emit a native notification with the reason.
- **R10.** Tray Stop calls `stop_recording` directly in Rust. For multi-speaker, `tray_model` accepts a `multi_speaker` flag and returns the Electron submenu shape (2 / 3 / 4+ / Skip). The payload plumbing (`numSpeakers`, `skipDiarization`) is spec 02's, which lands before this spec. Wire `multi_speaker` to 02's `MULTI_SPEAKER` setting and pass 02's stop arguments directly.
- **R11.** If tray creation fails or no icon loads, the app still runs (log and continue) and R1 falls back to quit-on-close (no hidden-window state without a tray).

Notifications (P2)
- **R12.** New setting `notify_summary_complete` (bool, default `false`, as in Electron), in Settings UI as a toggle with the same label as Electron's.
- **R13.** When a session transitions to `Done` in the job state machine (initial summary or regenerate) and the setting is on, post a native macOS notification with the generic text "Summary ready" and no session title (R15 spec author's call; gap-check fixed the earlier "with the session title" wording that contradicted R15). Triggering from Rust (not the renderer) fixes Electron's coupling to the open session view. Suppress it when the main window is visible and focused.
- **R14.** On first enable, request notification permission. If denied, show an inline hint in Settings with a button that opens System Settings > Notifications via the existing deep-link path. Don't crash or retry silently.
- **R15.** The notification text never includes transcript or summary content, only the title. Titles can carry client names. If the title looks sensitive, use the generic "Summary ready" (Spec author's call: always generic; reason: therapist clients, and banners show on lock screens).
- **R16.** Validation and error messages are not native notifications and not modal OS dialogs. They use in-app UI (belongs to each feature's spec). One exception: the R9 tray-start failure while the window is hidden, which shows the window (with the in-app error) and posts one notification.

Window chrome and navigation (P2)
- **R17.** Window config in `tauri.conf.json`: `titleBarStyle: "Overlay"`, `hiddenTitle: true`, min size 670x552, default 670x552 (Electron's `index.js:218-221`), `trafficLightPosition` about (9, 8) as in `index.js:244` if supported by the pinned Tauri (check at implementation). The window should not flash white: create hidden and show once the frontend signals ready.
- **R18.** The top 30px of the Svelte layout is a drag region (`data-tauri-drag-region`) with interactive controls excluded. Add `core:window:allow-start-dragging` to the capability, as the existing capability file lists only `core:event:default`.
- **R19.** Keep the default macOS app menu (Edit/Window) so Cmd+C/V/W/M/Q work. Add nothing custom.
- **R20.** Vibrancy / transparency (Electron `transparent`+`under-window`, `index.js:234-236`) is **P3, cosmetic**. It needs `macOSPrivateApi` and so is a risk to notarization and Mac App Store review. Default: skip it for v1 (Decision 3).
- **R21.** View navigation: add a small nav store in `src/lib` mirroring `viewNavigation.js`. It has views `record | settings | session-detail`, a `navigationLocked` flag, and settings loaded on entry. While models download (`ModelStatus` progress), the Settings button, back buttons and session list are disabled and visually dimmed. They are not locked during summarization. The `session-detail` view arrives with the sessions work (see gaps).

External links (P2)
- **R22.** `open_external_link(url: String)` is introduced by spec 04 (which lands first) in this shape; here, verify it matches and add only what is missing. It parses the URL with `reqwest::Url` (already a dependency; no new crate) and allows only `http`, `https`, `mailto`. Anything else returns an error without opening. It launches via `open` (same mechanism as the existing `open_privacy_settings`, `commands.rs:226`; check how that is done and reuse).
- **R23.** Block in-webview navigation to external origins (`on_navigation`: allow only the app origin and the dev URL). A global click handler routes `<a href>` with http(s)/mailto to R22 and prevents default. This is stricter than Electron (no guard today).
- **R24.** Wrapper `openExternalLink` in `src/lib/api.ts` (needed by the parity test).

Background animation (P3)
- **R25.** Setting `show_background_animation` (bool, default `true`), Settings toggle labelled as in Electron (`settings.js:57-70`). Changes apply immediately without a restart, including turning it back on after off (fix Electron's likely restart bug).
- **R26.** A `BackgroundAnimation.svelte` component: an in-house 2D-canvas starfield (≈ same visual: particles falling, idle slow, faster while recording). It pauses the rAF loop when the document is hidden or the setting is off, and respects `prefers-reduced-motion`. It runs faster when `current_recording` is set, driven by the same session status events the UI already uses.
- **R27.** No `three` dependency unless Patrick insists on the exact look (Decision 4).

Recording-limit reminder (CANDIDATE, not parity; P3, do not build in this spec unless Decision 5 says yes)
- **C1.** Settings: `long_recording_reminder_enabled` (default true), `long_recording_minutes` (default 55, range 10–240). The Rust recorder arms a timer at start (cancelled on stop). On expiry it emits `recording-limit-reached` and a native notification.
- **C2.** The frontend shows an in-app modal "Do you want to stop recording?" (Stop / Keep recording; ask again after another interval). A native notification can't carry buttons on macOS desktop in the official plugin, so the decision lives in-app (no `tauri-plugin-dialog`).
- **C3.** The hint "You can change this in Settings" is shown in the modal.

## Acceptance scenarios (numbered AS1..; Given/When/Then; each tagged automated or manual)
- **AS1 (automated, cargo test).** Given `tray_model(false,false)`, when built, it is [Show, sep, "Start Recording", sep, Quit]. With `recording=true` the record item is "Stop Recording". With `recording=true, multi_speaker=true` the item is replaced by the header plus 2 / 3 / 4+ / Skip entries.
- **AS2 (automated, fake-engines).** Given the fake recorder, when a recording starts, then stops (and also when the watchdog fails a source), then the tray-state observer receives true then false exactly once each.
- **AS3 (manual, real app).** Given the app has launched, then the menu bar shows the Scribular icon. Clicking it opens the menu; the icon does not toggle the window.
- **AS4 (manual).** Given the window is hidden, when I choose Start Recording, then recording starts, the icon turns red, the window stays hidden and the item reads Stop Recording. When I choose Stop Recording, then the session is transcribed and summarized and is visible when I reopen the window.
- **AS5 (manual).** Given the mic permission is denied, when I start from the tray, then the window comes forward and a notification states the reason.
- **AS6 (manual).** Given a recording, when I click the red close button, then the window hides and the recording continues (icon stays red). Choosing Show Scribular restores the same live window with the recording intact. Clicking the Dock icon also restores it.
- **AS7 (manual).** Given a recording, when I press Cmd+Q, then the app stays alive, the window is shown with the confirm modal; Cancel returns to recording. Confirm quits, and on relaunch the session exists as interrupted with its audio. Given idle, Cmd+Q quits at once.
- **AS8 (automated, fake-engines).** Given a recording and a `shutdown()` call, called twice, then the recording is abandoned once, the session is left `Recording` for next-launch recovery, and no panic occurs.
- **AS9 (automated, cargo test).** `notify_decision(setting, status_transition, window_focused)` returns true only for (on, →Done, not focused). It is false when the setting is off, when the transition is →Failed/Cancelled, or when the window is focused.
- **AS10 (manual, bundled build, not `tauri dev`).** Given the setting is on and I switch to another app while a summary is generating, then a macOS banner "Summary ready" appears when it finishes. It contains no summary text. With the setting off, nothing appears. (Dev builds attribute notifications to the terminal, so only the bundled app counts.)
- **AS11 (manual).** Given notification permission is denied, when I enable the toggle, then a hint with an Open System Settings button appears and the toggle does not claim success.
- **AS12 (automated, cargo test).** `open_external_link` accepts `https://www.scribular.com/#pricing` and `mailto:support@scribular.com`, and rejects `file:///etc/passwd`, `javascript:alert(1)`, `x-apple.systempreferences:...` and `not a url`, without launching anything (the launcher is injected).
- **AS13 (manual).** Clicking the release-notes link opens the default browser, and the app webview never navigates away.
- **AS14 (manual).** The window opens at 670x552, cannot shrink below it, and the title area drags the window. The traffic lights sit in the overlay and don't overlap content.
- **AS15 (automated, vitest).** The nav store disables Settings, back and session-list actions while `navigationLocked` is true, and does not lock when only summarization is active.
- **AS16 (manual).** Starting a model download disables the Settings button; completing it re-enables it.
- **AS17 (automated, vitest).** `BackgroundAnimation` does not schedule frames when the setting is off, and does when it is flipped on again (regression for Electron's `particles.js:165-173`).
- **AS18 (manual).** Toggling the animation off and on in Settings hides and shows it immediately. Activity Monitor shows near-zero GPU use with it off or with the window hidden.
- **AS19 (automated, cargo test).** `cargo test` passes the existing `every_command_has_an_api_wrapper` / `every_event_has_an_api_listener` after new commands and events are added.
- **AS20 (manual).** Hidden-window recording for 10+ minutes completes without gaps or throttling (check App Nap behaviour; a `caffeinate`-free test).
- Candidate only (C): **ASC1 (automated, fake clock)** a timer set to N minutes fires once at N, never after stop. **ASC2 (manual)** at the limit a notification and modal appear, and "Keep recording" re-arms.

## Design notes (modules, commands/events, data model + migration, UI components)
New and changed Rust:
- `src-tauri/src/shell/mod.rs` (new): `tray.rs` (`tray_model` pure, `install_tray` adapter), `quit.rs` (`QuitGuard`, `confirm_quit`), `links.rs` (`validate_url`, `open_external_link` with an injected launcher), `notify.rs` (`notify_decision` pure, a thin plugin call).
- `src-tauri/src/lib.rs`: build the tray in `.setup`, add `.on_window_event` (CloseRequested → hide), `.build(...).run(|app, event| ...)` handling `ExitRequested` / `Exit` / `Reopen`, and register the new commands.
- `src-tauri/src/jobs/mod.rs`, `session.rs`: add an observer hook next to `JobEvents` (a `RecordingObserver` trait, or reuse the existing event path). The tray and the notifier subscribe to status changes. Add an idempotent `Recorder::shutdown()` that `Drop` also calls.
- `src-tauri/src/store/settings.rs`: new `AppSettings { show_background_animation: bool, notify_summary_complete: bool }` (ts-rs), stored as keys in the existing `settings` table. No migration is needed because it is key/value and absent means default. If specs 03/06 also add general settings, keep a single `AppSettings` struct; the first spec to land creates it.
- Commands (`commands.rs`): `get_app_settings`, `set_app_settings`, `open_external_link`, `confirm_quit`. Events: `tray-start-recording`, `quit-requested`. Each gets a wrapper in `src/lib/api.ts` and a types entry via ts-rs (`src/lib/types`).
- `tauri.conf.json`: window config per R17. `capabilities/default.json`: add `core:window:allow-start-dragging` and `core:window:allow-show`/`allow-set-focus` if the frontend raises the window (prefer doing that in Rust).

Frontend:
- `src/lib/nav.ts` (store) and `App.svelte` use it (R21).
- `src/lib/components/Titlebar.svelte` (drag region plus the Settings button).
- `BackgroundAnimation.svelte`, `QuitConfirm.svelte`.
- A Settings section with two toggles in `Settings.svelte`.
- A global link handler in `main.ts`.

Why the tray runs from Rust state: Electron's renderer→main round trip (`RECORDING_STATE_CHANGED`) exists because main didn't own recording state; Tauri's core does, so a window that is hidden or reloaded can't leave the tray stale.

Why hide-on-close: the tray exists so recording can continue without a window, and Electron's close-kills-recording behaviour is a trap (`audio.js:671-688` runs on `window-all-closed`).

App Nap: a hidden-window recording relies on active audio I/O to keep the process awake; hold an activity assertion only if AS20 shows throttling (would need a crate, so a separate review).

## New dependencies (candidates + review needed)
Dependency policy check only (R0); no review doc.
1. **`tauri` feature `tray-icon`** (plus `image-png` for `Image::from_bytes`). Built-in, official; pulls the `tray-icon` crate (and `muda`, already used for menus). The in-house alternative is none (it needs AppKit `NSStatusItem`). Needed for R5-R8.
2. **`tauri-plugin-notification`** (official, Rust crate only; call it from Rust so the JS package `@tauri-apps/plugin-notification` is not needed). Needed for R13 and R9 failure notices. Alternative: `osascript -e 'display notification'` through `std::process::Command` costs no dependency but is attributed to Script Editor, shows the wrong icon, and can't request permission. Rejected for a clinical product's polish, but it is the fallback if the plugin fails the dependency policy check.
3. **Not added (in-house instead):** `tauri-plugin-opener` (R22 uses `reqwest::Url` plus the `open` command), `tauri-plugin-dialog` (in-app modals), `three` (R26 uses canvas 2D), `tauri-plugin-window-state` (Electron doesn't persist bounds).
4. Possible later: an `objc2` crate for App Nap or vibrancy, only if AS20 or Decision 3 demands it.

## Out of scope
- Windows/Linux window controls and resize handles; `WINDOW_MINIMIZE` / `WINDOW_CLOSE` / `WINDOW_RESIZE` / `WINDOW_GET_BOUNDS` IPC.
- A splash screen (Electron ships an empty one, unused).
- The multi-speaker prompt and payload handling (spec 02).
- Licence-restricted start blocking (spec 04); error/analytics-on-quit (spec 06); update-ready banner/link (spec 05); crash-recovery UI for interrupted sessions (spec 09).
- Persisting window bounds; custom app menus.

## Depends on / interacts with (other parity specs by slug)
- 02-multi-speaker-diarization: tray stop submenu payload (R10).
- 04-licensing-trial: tray Start must honour restriction (R9).
- 05-auto-update: release-notes link uses R22; update restart should call the same `shutdown()` (R4); the quit-confirm should also guard `install_and_relaunch` during a recording.
- 06-diagnostics-telemetry: Electron sends analytics on quit (`index.js:573-580`); any Tauri equivalent hooks into `RunEvent::Exit`.
- 09-session-management-recovery: interrupted-session recovery after a confirmed quit (R3).
- 07-onboarding-samples: onboarding may reuse the notification-permission hint (R14).

## Coordination notes (gap-check 2026-11-04)
- **Settings layout is owned here (gap assigned by gap-check).** Electron's Settings has a category sidebar: Note styles, Session, General, About (`index.html:145-168`, panels `:466/:520/:569/:657`; switching `sidebar.js:70-99`), plus a hidden License panel opened from the badge (`index.html:693`, `settings.js:561`). Restructure `Settings.svelte` into these categories (License as its own category or under General) and move in the headed sections earlier specs added: 01 keep-audio, 02 multi-speaker, 03 note styles + model, 04 licence, 05 updates, 06 privacy/diagnostics, 07 restore sample. Add AS: "each category shows its sections; switching categories keeps unsaved-state guards (03 R13)". About shows version (`app_version`) and support links via R22. The Settings deep link (Electron `client.js:271`) maps to the nav store opening a given category.
- **Already landed before this spec:** 01 (tray Start hidden while Locked), 02 (R10 wiring), 04 (`open_external_link`, R9 gate), 05 (`UpdateBar`; quit paths reach `RunEvent::ExitRequested`), 07 (Help → "Show tour" menu entry is added here).
- **Session detail:** R21's `session-detail` view is filled by 09. Here, add the view to the nav store and keep `navigationLocked` behaviour for it.
- **Notifications:** `tauri-plugin-notification` is the only native notification path. Feature errors stay in-app (R16).

## Decisions for Patrick (with recommended default)
1. **Window close while recording.** Electron destroys the window and kills the audio worker (`index.js:592-599`). *Default:* hide the window and keep recording (R1), with a confirm on Cmd+Q (R3).
2. **Native banner vs modal for "summary complete".** Electron shows a blocking OS modal. *Default:* a native banner, opt-in and off by default, generic text without client names (R12-R15).
3. **Vibrancy/transparent window.** It needs `macOSPrivateApi`. *Default:* skip for v1 (R20); revisit if the flat look is a complaint.
4. **Starfield implementation.** *Default:* in-house canvas 2D, no `three` (R26-R27). If the exact 3D look matters, add `three` (Patrick's call; the dependency policy check is enough).
5. **55-minute reminder (Electron doesn't ship it).** *Default:* defer. Don't build C1-C3 in this PR. Pick it up after parity, with limit default 55 min and an in-app modal.
6. **Notify when the window is focused.** *Default:* suppress (R13).

## Size (S/M/L, rough days) and suggested PR split if > ~1 PR
**M, about 6-8 days** (the 55-min candidate would add about 1 day).
1. PR1: dependency policy check (R0), `AppSettings`, and the Settings toggles, with the tests.
2. PR2: quit/close lifecycle (R1-R4) plus `Recorder::shutdown()`.
3. PR3: tray (R5-R11), the largest and most manual testing.
4. PR4: notifications (R12-R16).
5. PR5: window chrome, nav store and external links (R17-R24).
6. PR6: background animation (R25-R27), P3, and optional C1-C3.
