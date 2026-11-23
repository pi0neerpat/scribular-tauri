# Keyboard shortcuts PR2: AS7 fix (global chord does nothing)

Status: requested by Patrick in product review of PR 52 (2026-11-21).

## Problem

Product review: with background shortcuts enabled, neither the default chord
(`control+alt+super+KeyR`) nor a custom chord starts or stops recording.

Cause: `src-tauri/src/lib.rs:138` builds `tauri_plugin_global_shortcut::Builder::new().build()`
with no handler, so registration succeeds but a press does nothing. The spec's split put the
toggle in PR3 (R10/AS8), yet AS7 ("the app responds only to an active registration") cannot pass
without it. AS7 was also deferred to Patrick as "manual real Mac" although uitest can drive it:
`key` posts CGEvents to `.cghidEventTap` (system-wide), and `close-window`, `quit`,
`launch` with kept data, `play`, and `expect-status-item` already exist.

## Change (PR2 scope)

1. Pressed toggle (minimal R10 core). Add `.with_handler` to the global-shortcut builder. Act
   only on `ShortcutState::Pressed` for the chord that `shortcuts::active(app)` reports. Suppress
   held-key repeats and a second press within 1 s. If recording, stop through the tray stop path
   (`shell::tray::stop`). Otherwise emit `TRAY_START_RECORDING_EVENT`, the same event and guards as
   the tray Start item. Do not show or focus the window. Success notifications and the full
   race/notification coordinator stay in PR3 (AS8–AS12).
2. Unit test: debounce/repeat/Released handling as a pure function in `shortcuts.rs`.
3. uitest (uitest first, human review last). New scenario
   `tools/uitest/scenarios/as7-background-shortcut.uitest`, built from `system-audio-smoke.uitest`
   (App audio + `play`, never the microphone):
   - enable background shortcuts in Settings › Keyboard Shortcuts and assert the consent text;
   - `close-window` (app hidden), `key ctrl+option+cmd+r`, assert tray shows recording;
   - `key ctrl+option+cmd+r` again, assert tray shows not recording;
   - `quit`, relaunch with kept data, press the chord, assert recording starts; press it again to
     stop;
   - set a custom chord, press the old chord (no change), press the new chord (toggles).
   If a step needs a command uitest lacks (for example a key code, or asserting that Scribular is
   not frontmost), add it: expansion only, with a `UITestCoreTests` test. Never edit existing
   commands, scenarios or assertions.
4. Only the competing-app half of AS7 (another app claiming the chord) stays `manual-only`: it
   needs a third-party app. Record it with `cwf handoff --unverified "AS7b: manual-only: ..."`.

## Done when

- Rust, vitest, `npm run check` and the uitest suite pass on the new head.
- `as7-background-shortcut.uitest` passes against the debug bundle and is recorded as a suite.
- The PR body lists AS7a as automated and only AS7b as manual.
