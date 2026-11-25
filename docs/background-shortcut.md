# Background recording shortcut

Opt-in global chord that starts or stops recording while Scribular is in the background (keyboard shortcuts spec, parts 2 and 3).

## What it does
- Settings > Keyboard Shortcuts: switch on the background shortcut, capture a chord, see the active chord. Help > Keyboard Shortcuts opens the pane. The pane discloses that the chord works in every app and that recording needs client consent.
- A press on the active chord toggles recording through the same start/stop path as the tray. Key repeat is ignored and presses within one second of the last toggle are ignored. Only one start or stop runs at a time; a press during an in-flight or pending action is dropped.
- The app stays in the background. A chord start shows a "Recording started" notice with the note style name; a completed chord stop shows "Your note is being written."
- The tray menu label shows the active chord with macOS modifier glyphs (for example ⌃⌥⌘R). The tray tooltip reads "Scribular" or "Scribular — Recording".
- A failed stop keeps the recording recoverable (the session stays in History) and shows a plain failure notice. Notices carry only a fixed message, never error text, paths or note content.
- The note style is snapshotted when Stop is pressed, so a style picked during the recording is the one used.

## How it works
- `src-tauri/src/shortcuts.rs`: `validate` normalises a chord, `set` registers a replacement and keeps the previous chord if registration fails, `restore` re-registers on startup, `unregister` removes it, `toggle_on_press` handles presses. State is emitted on `shortcut://registration`.
- `src-tauri/src/store/settings.rs`: `global_shortcuts_enabled` (default off) and `toggle_recording_shortcut` live in the encrypted settings database. `ShortcutSettings` is exposed to TypeScript (`src/lib/types/ShortcutSettings.ts`, `src/lib/api.ts`).
- `src-tauri/src/shell/recording.rs`: one `Transition` guard serialises tray and chord actions. A start is handed to the webview as a request-id'd `tray-start-recording` event; the id expires after 10s, and the webview calls `cancelTrayStart` (`src/lib/components/RecordView.svelte`) for any request it cannot handle (already busy) or has finished. `finish_stop` maps stop errors to fixed notices, logs only an error kind, and emits `tray-stop-failed` (handled in `src/App.svelte`) so the foreground shows the failure. Notices fire only for a completed or failed transition, never for a refused one.
- `src-tauri/src/shell/tray.rs`: `display_chord` builds the glyph label; `refresh` sets menu, tooltip and icon. `shortcuts::refresh_tray` re-syncs after a chord change.
- `src-tauri/src/jobs/session.rs`: `stop_with_speakers` calls `snapshot_recording_prompt` before taking the recording; if that fails it returns `StopPreparationFailed` and the recording keeps running.
- The speaker-count prompt in `RecordView.svelte` is cleared (`askSpeakerCount = false`) when progress leaves `recording`, so a chord stop does not leave it open.
- UI: `src/lib/components/ShortcutsSettings.svelte`. Escape cancels capture, Delete clears the chord (inactive), Restore Defaults reactivates the default.

## Test
- `cargo test --manifest-path src-tauri/Cargo.toml --lib`, `npm test`, `npm run check`.
- Unit: `cargo test --manifest-path src-tauri/Cargo.toml --lib shell::` covers the transition guard, stop-failure notices and chord label (AS8, AS12); `src/lib/components/record-view-speaker-count.svelte.test.ts` covers the prompt.
- Native: `tools/uitest/scenarios/as7-background-shortcut.uitest` (App audio fixture, `SCRIBULAR_SKIP_LICENSE=1` on a debug bundle) and `tools/uitest/scenarios/keyboard-pr3-*.uitest` (background, paused, call-mode long audio, registration fallback, style at stop, unavailable hotkey).

## Known limits
- AS7a native run is unverified on the merged head: `pick-call-app` times out in AudioCapCLI source listing (pre-existing, tracked separately).
- AS7b (another app already owns the chord) is manual-only; macOS may reject or share the chord.
- AS10 tray-start fallback (`keyboard-pr3-unavailable-hotkey-tray-start.uitest`) could not be run reliably: Control Center covers the status item. Patrick waived it; component tests cover the fallback.
- uitest did not observe macOS notification banners; notices are covered by unit tests only.
- The room-mic scenarios (`keyboard-pr3-*-mic-*`) were superseded by the call-mode scenario (On a call plus `afplay` App audio).
