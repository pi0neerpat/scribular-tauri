# Plan 5 Phase B: uitest native surfaces (scribular-tauri, Swift)

Scope: `tools/uitest/` only (plus nothing in product code; see R6). Start after Plan 4 (recording/pacing, PR #21) is merged; this spec uses Plan 4's `--record <name>` / `--pace` / `pause` but must not change them. **EXPANSION ONLY**: new commands, flags, scenarios and tests; no change to existing command behaviour, existing scenarios, or existing test assertions. Enforced by `tools/uitest/scripts/check-additive.sh` (R8).

Source of truth for every AX path and command below: Phase A spike note `plans/evidence/uitest-native-spike.md` (clone `scribular-tauri-p11`, rev `37582c8`, 2026-11-08). Only capabilities the spike proved (yes or partial) are kept; disproved or unproven parts are listed in Out of scope with the spike's reason.

## Evidence

- Spec 08 (PR #18): 13 acceptance scenarios shipped "unverified"; AS3 "UI runner cannot locate or click the separate tray status item/menu"; the tray PNGs were fully transparent and Patrick found that by hand ("I dint see any system tray icon", logs/prompt-0010.txt); AS4 (hidden-window tray recording), AS5 (mic denial notification), AS6 (hide/restore), AS7 (Cmd+Q modal) unexercised.
- Spec 09: AS14-18 blocked by a keychain prompt (runs/2026-11-06-084408-212/fail-1.png).
- Spec 06: AS12 (Developer-section controls hidden by the production frontend), AS13 (AudioCapCLI audio-device error).
- `AXDriver` searches only the app's windows; the recorder captures the whole screen, so the menu bar is visible in video but not clickable.
- Spike result table:

| Capability | Spike verdict | Phase B |
| --- | --- | --- |
| 1. Status item + menu items | Partial: status item found and `AXPress` works; menu items NOT in AX; SystemUIServer fallback `-25212` | R1 narrowed to status item only |
| 2. Menu-bar strip + icon check | Partial: strip capture works; contrast check uncalibrated (9 luma delta) | R2 screenshots kept; `assert-icon-visible` deferred |
| 3. Notification Center banner AX | Partial: `osascript` banner read via AX; app notifications not triggered (permission unanswered) | R3 kept; app-notification scenario gated on Patrick |
| 4. Dock restore + Cmd+Q | Partial: Dock `AXDockItem` press restores hidden window; idle Cmd+Q exits; recording-active modal untested | R4 `dock click` kept; `expect-modal` deferred |
| 5. Keychain after rebuild | Yes | R5 kept |
| 6. Developer controls | Yes via signed debug bundle + dev-mode frontend; runtime env cannot work | R6 = build recipe, no product change |

## Host facts for the builder

- VM: macOS 26.2 (25C56), arm64. Prefix commands with `PATH=/opt/homebrew/bin:/Users/admin/.local/bin:$PATH`.
- Bundle id `com.app.scribular.tauri`; signing identity `Scribular Local Dev`.
- Release build (from repo root):
  ```sh
  CI=1 npm run tauri -- signer generate --ci -w <scratch>/updater.key   # once, outside the repo
  CARGO_TARGET_DIR=/Users/admin/cargo-target/<clone> APPLE_SIGNING_IDENTITY='Scribular Local Dev' \
    TAURI_SIGNING_PRIVATE_KEY=<scratch>/updater.key npm run tauri -- build --bundles app
  ```
  App: `$CARGO_TARGET_DIR/release/bundle/macos/Scribular.app`. ~4 min.
- **Pre-existing failure, do not fix:** the build exits 1 *after* bundling because `src-tauri/tauri.conf.json:60` contains `REPLACE_WITH_TAURI_UPDATER_PUBKEY` (updater signer: `Invalid symbol 95, offset 7`). The `.app` is signed and usable. Do not edit `tauri.conf.json`.
- After every rebuild: `bash tools/uitest/trust-build.sh <path>/Scribular.app` (trusts the new cdhash for the Scribular keychain items). The login password is already stored (`security find-generic-password -s scribular-uitest -a login-password >/dev/null && echo stored` prints `stored`); one-time setup is `bash tools/uitest/trust-build.sh --store-password` (Patrick only).
- Host permissions verified: `AXIsProcessTrusted=true`, `CGPreflightScreenCaptureAccess=true`, Apple Events to System Events granted. Main display strip was 2560 px wide.
- Run a scenario: `swift run --package-path tools/uitest uitest <scenario> --app <path>/Scribular.app` (plus Plan 4 flags). Unit tests: `swift test --package-path tools/uitest`.
- Parser lives in `Sources/UITestCore/Script.swift` (`enum Step`); AX/CG side effects in `Sources/uitest/AXDriver.swift`; preflight in `Sources/uitest/Preflight.swift`; CLI in `Sources/uitest/main.swift`. `key` already supports `escape`.
- New steps that wait (`expect-status-item`, `expect-notification`) accept the existing `timeout Ns` suffix (extend the suffix's allowed-step list additively; existing steps keep their current behaviour).

## Requirements

- **R1 (narrowed) status item.** New commands:
  - `expect-status-item "<help>"`: wait until the app has a status item whose `AXHelp` equals `<help>`.
  - `status-item click "<help>"`: find it and perform `AXPress`.
  - Exact AX path (proven, `AXPress` returned 0):
    `AXUIElementCreateApplication(pid) → "AXExtrasMenuBar" → AXChildren → AXMenuBarItem[AXHelp="Scribular"]`
    (the spike writes this as `AXExtrasMenuBar → AXMenuBar → AXMenuBarItem`: the `AXExtrasMenuBar` attribute value *is* the role-`AXMenuBar` element, so read its `AXChildren` directly; title is empty; observed frame `(2074,3,36,24)`). Match on `AXHelp`, not title. Reuse the spike probe logic:
    ```swift
    let root = AXUIElementCreateApplication(app.processIdentifier)
    AXUIElementCopyAttributeValue(root, "AXExtrasMenuBar" as CFString, &raw)       // must be .success
    AXUIElementCopyAttributeValue(extras, "AXChildren" as CFString, &rawChildren)
    // first child with AXHelp == label → AXUIElementPerformAction(item, "AXPress")
    ```
  - Failure message when not found must list the `AXHelp` values that were present.
  - The opened menu is a native menu whose items are NOT exposed in AX (`AXMenu` nil; one child with empty role/title). Assert on it only via `screenshot-screen` (R2). Close it with the existing `key escape`; the builder must confirm on the VM that this closes the menu, else use a second `status-item click`.
  - No SystemUIServer fallback (spike: `AXMenuBar` there returned `-25212` / `kAXErrorNoValue`).
- **R2 (narrowed) surface screenshots.**
  - `screenshot-menubar ["name"]`: `screencapture -x -R0,0,<main display width in points>,40 <run dir>/<name>.png` (default name `menubar-<line>`). Spike used `-R0,0,2560,40`; `sips` reported 2560×40. Print the path like the existing `screenshot`.
  - `screenshot-screen ["name"]`: `screencapture -x <run dir>/<name>.png` (whole main display, default `screen-<line>`). Use for the opened tray menu and Notification Center.
  - Same run-dir/naming conventions as the existing `screenshot`; existing `screenshot` unchanged.
- **R3 (narrowed) `expect-notification "<text>"`.** Poll the Notification Center process (`com.apple.notificationcenterui`, process title "Notification Center") every ≤200 ms; pass when any `AXStaticText` `AXValue` in its window tree contains `<text>`. Proven path for a banner body:
  `AXApplication[title="Notification Center"] → AXWindow[title="Notification Center"] → AXGroup → AXGroup → AXScrollArea → AXGroup → AXGroup[description="<app>, <title>, <body>"] → AXStaticText[value="<body>"]`
  Do a recursive search over `AXChildren` rather than hard-coding depth (the permission banner sat at `AXWindow → … → AXGroup → AXStaticText`). Banners lived 3.0-4.4 s on the VM, so the step must start polling immediately and its default timeout must be ≥10 s; README must warn that the step has to follow the trigger directly. Only banner text is proven; Notification Center history/list is not.
- **R4 (narrowed) `dock click "<title>"`.** Find the Dock process (`com.apple.dock`), recursive search for role `AXDockItem` with `AXTitle == "<title>"`, perform `AXPress`. Proven: after existing `key cmd+w` the app stayed alive with 0 windows; `AXDockItem[title="Scribular"]` (1 match) `AXPress=0` restored a window (frame `945,229,670,552`). Fail if 0 or >1 matches, printing the matching titles. Existing `key cmd+q` is unchanged; no `expect-modal`.
- **R5 keychain + `uitest doctor`.**
  - New CLI subcommand `uitest doctor [--app <path>]` (does not run a script, does not change `uitest <script>` behaviour or the existing `Preflight.check`). It runs every check, prints one line per check `ok|MISSING <name>: <fix command>`, exits 0 when all pass, 1 otherwise. Checks:
    1. Accessibility: `AXIsProcessTrusted()` → fix: System Settings → Privacy & Security → Accessibility (host name as in existing preflight).
    2. Screen Recording: `CGPreflightScreenCaptureAccess()` → fix: … → Screen Recording.
    3. Automation (System Events), reported separately from 1-2: `osascript -e 'tell application "System Events" to get name of first process'` exits 0 → fix: … → Automation → allow System Events.
    4. `ffmpeg` on PATH → fix: `brew install ffmpeg`.
    5. Keychain password stored: `security find-generic-password -s scribular-uitest -a login-password` exits 0 (never print the password) → fix: `bash tools/uitest/trust-build.sh --store-password`.
    6. With `--app`: bundle exists → fix: the release build command above; and a reminder line `after each rebuild run: bash tools/uitest/trust-build.sh <app>` (per-build trust status is not detectable; spike did not prove a check for it, so print it as info, not pass/fail).
  - Put the report formatting/exit-code logic in `UITestCore` as a pure function over check results so it is unit-testable; the system calls stay in `Sources/uitest`.
  - `handle-keychain` behaviour unchanged; README documents the flow: store password once → `trust-build.sh <app>` after every rebuild → `handle-keychain` step as safety net (spike: `keychain-after-rebuild.uitest` PASS 3/3 and `handle-keychain` printed `no prompt`; the Always-Allow branch was not exercised).
- **R6 DEV exposure (AS12) via debug build, no product change.** Add `tools/uitest/tauri-debug-overlay.json`:
  ```json
  { "build": { "beforeBuildCommand": "NODE_ENV=development npm run build -- --mode development" },
    "bundle": { "createUpdaterArtifacts": false } }
  ```
  Build (from repo root):
  ```sh
  CARGO_TARGET_DIR=/Users/admin/cargo-target/<clone> APPLE_SIGNING_IDENTITY='Scribular Local Dev' \
    npm run tauri -- build --debug --bundles app --config tools/uitest/tauri-debug-overlay.json
  ```
  App: `$CARGO_TARGET_DIR/debug/bundle/macos/Scribular.app` (~6 min); run `trust-build.sh` on it. Reason (spike §6): the Developer section is gated by compile-time `import.meta.env.DEV` (`src/lib/components/Settings.svelte:369`) and the test commands by `#[cfg(debug_assertions)]` (`src-tauri/src/lib.rs:141-144`, `src-tauri/src/commands.rs:286-294`); a launch-time env var cannot change either. `SCRIBULAR_EXPOSE_DEV` is dropped. **Must not change the production default:** no edits to `Settings.svelte`, `lib.rs`, `commands.rs` or `tauri.conf.json`; the release bundle must still show no "Developer" section. Overlay keys must be verified against the Tauri 2 config schema when building.
- **R7 (narrowed) new scenarios** in `tools/uitest/scenarios/` (new files only):
  - `tray-status-item.uitest`: `launch` → `expect-status-item "Scribular"` → `screenshot-menubar "tray-icon"` → `status-item click "Scribular"` → `screenshot-screen "tray-menu"` → `key escape` → `quit`.
  - `close-hides-dock-restore.uitest`: `launch` → `expect "<a main-window label>"` → `key cmd+w` → `expect-not "<same label>"` → `dock click "Scribular"` → `expect "<same label>"` → `quit`. (Pick the label with `labels`.)
  - `notification-probe.uitest`: `launch --keep-data` → `expect-notification "uitest-notification-probe" timeout 30s` → `screenshot-screen "notification"` → `quit`; driven by an external `osascript` trigger (see Acceptance). Proves R3 without depending on app notifications.
  - `notification-summary-ready.uitest`: app-generated notification. **Gated:** requires Patrick to allow Scribular in System Settings → Notifications. If not granted, raise `needs_human` with that exact instruction and report the scenario as unverified; do not fake it.
  - `dev-controls-debug.uitest`: open Settings → General → `expect "Developer"`, `expect "Send test error"`, `expect "Trigger test panic"` (run against the R6 debug bundle only).
  - `dev-controls-release.uitest`: same navigation → `expect "Privacy & diagnostics"`, `expect-not "Developer"` (release bundle; guards the production default).
- **R8 `tools/uitest/scripts/check-additive.sh <base-ref>`** (as written in Plan 05): fails when the diff modifies or deletes lines in `tools/uitest/Tests/**` or existing `tools/uitest/scenarios/*.uitest` (additions and new files only), or changes the documented behaviour table rows of existing commands in README. Run in CI and by the reviewer. Implementation note: `git diff --numstat` / `--diff-filter` against `<base-ref>`; deleted or modified lines in those paths → exit 1 with the offending file; README check compares the existing command rows (`| \`launch …` through `| \`quit\` |` plus any rows Plan 4 added) verbatim. Include a self-test (bash, run in CI) that builds a throwaway git repo with one additive and one modifying commit and asserts exit 0 / 1.
- **R9 README** (`tools/uitest/README.md`, additions only): new rows for `expect-status-item`, `status-item click`, `screenshot-menubar`, `screenshot-screen`, `expect-notification`, `dock click`; a `uitest doctor` section; the keychain flow (R5); the debug-build DEV recipe (R6); the notification-lifetime warning (R3); the fallback ladder: UI command → surface screenshot → log/state assertion → simulated → unverified, with the current native-menu status stated (status item: UI command; menu items: surface screenshot only).

## Acceptance

Unit (`swift test --package-path tools/uitest`, all existing tests unchanged and green):
- AS1 `ScriptTests` additions: each new command parses to its `Step` (`expectStatusItem`, `statusItemClick`, `screenshotMenubar(name:)` with and without name, `screenshotScreen(name:)`, `expectNotification`, `dockClick`), `timeout 30s` suffix accepted on `expect-status-item` and `expect-notification`, and malformed lines (missing quotes, unknown subcommand e.g. `status-item hover`, `dock press`) fail with a line-numbered error.
- AS2 doctor report tests: all-ok → exit 0; any missing → exit 1 and the line contains the fix command; the password value never appears in output.
- AS3 `check-additive.sh` self-test passes (additive → 0, modified test/scenario/README row → 1).

Real runs on the VM (release build of current `main`, after `trust-build.sh`):
- AS4 `uitest doctor --app <release app>` prints all `ok`, exit 0.
- AS5 `tray-status-item.uitest` passes; `tray-icon.png` is 40 px tall and shows the Scribular glyph; `tray-menu.png` shows `Show Scribular`, `Start Recording`, `Quit` (reviewer checks the image; not AX-asserted).
- AS6 `close-hides-dock-restore.uitest` passes.
- AS7 `(sleep 8; osascript -e 'display notification "uitest-notification-probe" with title "uitest"') & swift run --package-path tools/uitest uitest tools/uitest/scenarios/notification-probe.uitest --app <release app>` passes.
- AS8 `dev-controls-release.uitest` passes on the release bundle and `dev-controls-debug.uitest` passes on the R6 debug bundle.
- AS9 `notification-summary-ready.uitest`: passes if Patrick granted Notifications; otherwise reported as unverified with the `needs_human` record.
- AS10 `keychain-after-rebuild.uitest` (existing, unchanged) still passes after the rebuild.
- AS11 `bash tools/uitest/scripts/check-additive.sh origin/main` passes on the PR branch.
- Happy-path recording (Plan 4 flag): `uitest tools/uitest/scenarios/tray-status-item.uitest --app <release app> --record 05-uitest-native-surfaces-happy-path` → `05-uitest-native-surfaces-happy-path.mp4` (+ `.json`) in the Plan 4 record dir, cited in the PR.

## Out of scope

- Windows; changes to existing commands, scenarios or test assertions; any product code (`src/`, `src-tauri/`), including `tauri.conf.json` and its updater pubkey.
- `menu-item click|expect "<label>"`: spike showed tray menu items are not exposed in AX (`AXMenu` nil) and the SystemUIServer fallback returns `-25212`; a coordinate/keyboard attempt at Start Recording did not reliably hit a row or start recording. Revisit only after a separately validated screenshot/keyboard path.
- `tray-start-stop.uitest` ("Start Recording" → menu shows "Stop Recording"): depends on menu-item commands; recording was never started in the spike.
- `assert-icon-visible`: positive passed and a transparent PNG failed, but measured contrast was only 9 luma points and the "background shows through a transparent template icon" case is inferred, not tested. Defer until calibrated against a real menu-bar screenshot with a transparent icon. Until then the icon is checked by `screenshot-menubar` + reviewer.
- `expect-modal` and `cmdq-confirm.uitest`: the recording-active Cmd+Q modal was never shown (recording did not start); idle Cmd+Q simply exits.
- `close-hides-keeps-recording.uitest`: recording state unverified; replaced by `close-hides-dock-restore.uitest`.
- Microphone-denial notification (AS5 of spec 08): needs `tccutil reset Microphone com.app.scribular.tauri` + manual "Don't Allow"; not run in the spike. Not in this plan.
- `SCRIBULAR_EXPOSE_DEV` runtime env and any production test-mode path for DEV controls: `import.meta.env.DEV` is compile-time.
- Detecting per-build keychain trust in `doctor`; the `handle-keychain` Always-Allow branch (no prompt appeared to exercise it).
- AudioCapCLI audio-device error (spec 06 AS13): not covered by the spike.
