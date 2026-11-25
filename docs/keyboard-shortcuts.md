# Keyboard shortcuts (part 1: fixed app keys)

Shipped in PR #41. Spec: `plans/2026-11-16-keyboard-shortcuts-spec.md` (R1-R4, AS1-AS3). Parts 2 and 3 (editable global recording key, Shortcuts pane, Help route) are shipped: see `docs/background-shortcut.md`. macOS only.

## What it does

- Native menu items show their accelerators: File (⌘N, ⌘R, ⌥⌘R, ⌥⌘N, ⇧⌘C, ⌘W), Edit (undo/redo, cut/copy/paste/select all, ⌘⌫, ⌘F), View (⌘1/2/3, ⌥⌘S, ⌥⌘↑/↓, ⌃⌘S, ⌃⌘F), Window (⌘M, ⌘0), app (⌘, ⌘H ⌥⌘H ⌘Q), Help (⌘?).
- Record and Pause menu items switch between Start/Stop and Pause/Resume and enable or disable with recorder state, including changes made from the tray or record screen.
- Shortcuts run the same owner action as the button (record, pause, copy note, delete session with its confirmation, find, tab and session navigation, sidebar toggle). A disabled key has no effect.
- Context keys: sidebar sessions take ↑/↓ and Return; Esc and ⌘. dismiss an open sheet or dialog and never cancel a recording.
- Shortcuts are suppressed while any modal or dialog is showing. The floating onboarding tour does not block them.

## How it works

- `src-tauri/src/shell/menu.rs`: `BINDINGS` is the single table of menu id to accelerator, used to build the menu. `set_state` and `sync_recording_state` update labels and enablement from owner state and live recorder events. `handle` emits the `shell://menu` event.
- Frontend handlers in `src/App.svelte` and the record, session and sidebar components receive the event and call the owning action.
- Test: `cargo test --manifest-path src-tauri/Cargo.toml --lib recording_labels_and_availability_follow_owner_state` (AS2). AS1 and AS3 are manual real-app checks.

## Known limits

- Note tabs, persistent note undo/redo and Help ▸ Keyboard Shortcuts stay disabled until later work.
- Menu accelerators are fixed; only the global recording chord is editable.
- AS1 and AS3 were not verified by an agent.
