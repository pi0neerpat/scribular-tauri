# Background recording shortcut

Opt-in global chord that starts or stops recording while Scribular is in the background (keyboard shortcuts spec, part 2).

## What it does
- Settings > Keyboard Shortcuts: switch on the background shortcut, capture a chord, see the active chord. Help > Keyboard Shortcuts opens the pane. The pane discloses that the chord works in every app and that recording needs client consent.
- A press on the active chord toggles recording through the existing tray start/stop paths. Key repeat is ignored and presses within one second of the last toggle are ignored.
- The tray shows the active chord.

## How it works
- `src-tauri/src/shortcuts.rs`: `validate` normalises a chord, `set` registers a replacement and keeps the previous chord if registration fails, `restore` re-registers on startup, `unregister` removes it, `toggle_on_press` handles presses. State is emitted on `shortcut://registration`.
- `src-tauri/src/store/settings.rs`: `global_shortcuts_enabled` (default off) and `toggle_recording_shortcut` live in the encrypted settings database. `ShortcutSettings` is exposed to TypeScript (`src/lib/types/ShortcutSettings.ts`, `src/lib/api.ts`).
- UI: `src/lib/components/ShortcutsSettings.svelte`. Escape cancels capture, Delete clears the chord (inactive), Restore Defaults reactivates the default.

## Test
- `cargo test --manifest-path src-tauri/Cargo.toml --lib`, `npm test`, `npm run check`.
- Native: `tools/uitest/scenarios/as7-background-shortcut.uitest` (App audio fixture, `SCRIBULAR_SKIP_LICENSE=1` on a debug bundle).

## Known limits
- AS7a native run is unverified on the merged head: `pick-call-app` times out in AudioCapCLI source listing (pre-existing, tracked separately).
- AS7b (another app already owns the chord) is manual-only; macOS may reject or share the chord.
