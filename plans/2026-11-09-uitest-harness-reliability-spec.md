# uitest harness reliability — spec (2026-11-09)

Harness-only change: `tools/uitest` (Swift sources, tests, README, scenarios). No app code
(`src/`, `src-tauri/`) changes. GUI verification reuses the existing release bundle
`/Users/admin/cargo-target/scribular-tauri/release/bundle/macos/Scribular.app`; no Rust rebuild.

## Problem (evidence)

Investigation on 2026-11-09 (VM, `/Users/admin/Projects/Scribular/scribular-tauri-coach-p1/runs/20261109T_investigate/`):

- **Covered-click false pass.** `occl-old/uitest.log`: with other windows (Activity Monitor,
  Brave, Apple menu) in front of Scribular, step 16 `click "Stop"` reported ✓ but the click
  landed on the covering window; the app kept recording (`"Recording… 1:26"`) and step 18
  failed 60 s later with a misleading error. `occl-new3/uitest.log`: same scenario with the
  fixed harness passes 23/23 because the covered target is pressed through Accessibility.
- **Concurrent runs.** Two agents drove the VM GUI at once; their clicks interleaved. A run lock
  makes the second run exit 2 (`lock-test.log`: `another uitest run is active (pid 31676`
  followed by a stray newline — the pid includes a trailing newline; fix it).
- **Weak failure evidence.** Failures saved only a window screenshot; the covering window and
  the frontmost app were not visible, and the app log was shared across runs.
- **Scenario flakiness.** Looped `play` clips overlap the next Start; live transcript phrases
  vary between Whisper runs; summaries on the VM can take > 300 s.

## Scope

1. Cherry-pick the three investigation commits (already on this branch: e68bf84, 4304242,
   6e4e617) that:
   - refuse clicks while a Keychain prompt is open;
   - hit-test the click point; if the target is covered, press it via AXPress and log
     `covered by <app>; pressed via AX`;
   - raise the app through Accessibility before guarded clicks;
   - on failure save a full-screen screenshot plus the frontmost app name;
   - copy each run's own app log into its run dir;
   - add a run lock: a second concurrent `uitest` exits 2 with `another uitest run is active (pid N)`.
2. Fix the lock-message nit: the pid must be trimmed (single line, `(pid 12345)`), with a unit test.
3. README "Scenario rules" section:
   - play a one-shot clip (not `loop`) right before each `Start recording`; a `loop` play is only
     for making `afplay` visible in the App picker and must be stopped/superseded before Start;
   - don't assert live-transcript phrases while recording; assert only phrases that are stable
     across Whisper runs (e.g. "since we last met") after Stop;
   - give the summary a generous timeout on slow VMs (>= 400 s);
   - run a clean-desktop / keychain-trust preflight before runs (no windows over Scribular, no
     other uitest/Scribular running, keychain trust set via `trust-build.sh`).
   Fix any scenario in `tools/uitest/scenarios/` that violates these rules (e.g. summary timeouts
   < 400 s).
4. Optional, only if small: a `uitest doctor` (or preflight at run start) that warns when other
   windows are in front of the target app or another uitest/Scribular process is running.
   Warning only; it must not fail runs.

Out of scope: app code, new scenario commands beyond `doctor`, changing existing command semantics
other than the click guard above.

## Acceptance scenarios

- **AS1** `swift test` in `tools/uitest` passes (>= 64 tests plus new ones).
- **AS2** A deliberately covered `click "Stop"` (another app window placed over Scribular) is
  pressed via AX, the log line says so, and the recording actually stops (scenario passes).
- **AS3** Starting a second `uitest` while one is running exits 2 with
  `another uitest run is active (pid N)` on one line.
- **AS4** A deliberately failing scenario saves the full-screen screenshot, the frontmost app name
  and the run's own `app.log` in its run dir.
- **AS5** A concurrent AS10-style scenario (record, stop, start a second recording while the first
  summary generates, stop, both summaries finish) passes with the fixed harness against the
  existing release bundle; a happy-path recording is kept under `runs/` (not committed).
- **AS6** Repo scenarios conform to the README scenario rules.

## Verification notes

- Only one agent may drive the VM GUI at a time. Before each GUI run check
  `pgrep -fl "uitest|Scribular.app|codex"` and close Brave/Terminal/Activity Monitor windows over
  Scribular (except where AS2 needs a deliberate cover).
- Disk is tight (~9 GB free): keep runs lean, one recording only for the final happy path.
