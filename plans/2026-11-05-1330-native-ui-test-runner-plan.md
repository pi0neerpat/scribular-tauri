# Native UI Test Runner Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Swift CLI `uitest` that runs a plain-text action script against the real built Scribular app with real mouse/keyboard input, prints per-step results for the agent, and saves a screen recording for the human.

**Architecture:** SwiftPM package at `tools/uitest/`. A pure, unit-tested core (`UITestCore`: script parser, label picking, step runner against a `Driver` protocol, output formatting). A thin macOS executable (`uitest`) implements `Driver` with the Accessibility API + `CGEvent`, and wraps the run with preflight, app launch, and `ffmpeg` recording.

**Tech Stack:** Swift 6.2 (Xcode 26.1.1 on this VM), swift-testing (`import Testing`), ApplicationServices (AX), CoreGraphics (`CGEvent`), `/usr/sbin/screencapture`, Homebrew `ffmpeg`.

**Spec:** `plans/2026-11-05-1300-native-ui-test-runner-design.md`

## Global Constraints

- macOS only; this VM (macOS 26.2, Apple Silicon). No new package dependencies.
- Keychain password only from env var `UITEST_KEYCHAIN_PASSWORD`; never printed, logged, or written to any file.
- Recordings and screenshots go to `runs/<timestamp>/` (timestamp `yyyy-MM-dd-HHmmss`), relative to the cwd; `runs/` is git-ignored. Nothing is uploaded.
- `launch` never builds. It prints the launched `.app` path and its build time (bundle executable mtime).
- App data dir `~/Library/Application Support/com.app.scribular.tauri` is **moved** into the run folder (`runs/<ts>/app-data`) before launch, never deleted, and deliberately not restored: each run starts from a clean app state, and the original data stays recoverable there (move it back manually). `runs/` holds sensitive data, so it must stay git-ignored; delete old run folders when no longer needed. `launch --keep-data` skips the move.
- Output contract (stdout), verbatim shape:
  ```
  ✓ 1 launch
  ✗ 6 expect "Bullet recap": not found after 10s
    screenshot: runs/<ts>/fail-6.png
    visible: "Settings", "Note styles", ...
  FAIL 5/6 steps — recording: runs/<ts>/recording.mp4
  ```
  Pass summary: `PASS 6/6 steps — recording: <path>`. Exit 0 on pass, 1 on step failure, 2 on parse/preflight error.
- Fail loudly rather than act on the wrong element: an ambiguous label is a failure, not a guess.
- App code changes allowed only for missing accessibility labels (`aria-label`).
- Work on branch `feat/native-ui-test-runner` off `main` in this repo. Commit after each task and `git push`.

## Review Focus

1. **Duplicate labels** (e.g. "Cancel" in the editor and in the discard dialog): with no index/scope, the step must fail listing the matches, never click the first one. → Task 2 test `pickAmbiguousFails`.
2. **Password leak:** the env value must not appear in any output line, even when `handle-keychain` fails. → Task 3 test `passwordNeverPrinted`.
3. **Data dir move edge cases:** dir absent (no-op, no error) and run folder already holding a `app-data` entry (fail before launching, nothing lost). → Task 5 tests `moveDataDirAbsentIsNoop`, `moveDataDirRefusesOverwrite`.
4. **Ctrl-C or crash mid-run:** recording is still a playable mp4 and the app is quit. → Task 5 manual check step.
5. **App already running from a previous run:** `launch` terminates it first so steps don't hit a stale window with old data. → Task 5 manual check step.

---

### Task 1: Spike — prove AX, keychain input, and recording on this VM (throwaway)

No production code. Result is a findings note appended to the spec and a go/no-go on the keychain path.

**Files:**
- Create (throwaway, not committed): `tools/uitest/spike/spike.swift`
- Modify: `plans/2026-11-05-1300-native-ui-test-runner-design.md` (append `## Spike findings`)

- [ ] **Step 1: Build the app with fake engines** in `../scribular-tauri-p03`:
  Run: `npm run tauri -- build --features fake-engines --bundles app`
  Expected: `src-tauri/target/release/bundle/macos/Scribular.app` exists. Record the exact working command in findings (adjust flags if this one is rejected).
- [ ] **Step 2: AX dump.** Open the app, go to Settings → Note styles, open a style editor. Run a `swift spike.swift` script that walks the app's AX tree (`AXUIElementCreateApplication(pid)`, recurse `kAXChildrenAttribute`) and prints role + `AXTitle`/`AXDescription`/`AXValue` for every element.
  Expected: labels for "Settings", "Note styles", new-style button, Save, the favourite toggle (`Toggle favourite for <name>`, `NoteStyles.svelte:196`), make-default, and the `Discard changes?` dialog (`NoteStyles.svelte:234`). List any control without a label.
- [ ] **Step 3: Keychain input.** Trigger the keychain prompt (rebuild, relaunch). In the spike, find the SecurityAgent process's window via AX, click its password field with `CGEvent`, post keystrokes for a test string, click "Always Allow".
  Expected: either the prompt is dismissed (→ keychain path = **type**), or keystrokes don't land / AX can't see the window (→ keychain path = **stable signing**, do Task 7).
- [ ] **Step 4: Recording.** Run: `ffmpeg -f avfoundation -list_devices true -i ""` then record 5 s: `ffmpeg -f avfoundation -framerate 15 -capture_cursor 1 -i "<screen index>:none" -t 5 /tmp/spike.mp4`
  Expected: playable mp4 showing the cursor. Record the screen index rule (name `Capture screen 0`) in findings.
- [ ] **Step 5: Write findings** into the spec under `## Spike findings`: build command, unlabeled controls, keychain path (type / stable signing), ffmpeg device name. Add `aria-label`s for any unlabeled controls in p03 as a separate commit there.
- [ ] **Step 6: Commit** the spec update (not the spike file).
  ```bash
  git add plans/2026-11-05-1300-native-ui-test-runner-design.md
  git commit -m "docs(plans): native UI runner spike findings" && git push
  ```

### Task 2: Package scaffold, script parser, label picking

**Files:**
- Create: `tools/uitest/Package.swift` (targets: library `UITestCore`, executable `uitest` depending on it, test target `UITestCoreTests`; platform `.macOS(.v14)`)
- Create: `tools/uitest/Sources/UITestCore/Script.swift`
- Create: `tools/uitest/Sources/UITestCore/Pick.swift`
- Create: `tools/uitest/Sources/uitest/main.swift` (placeholder: prints usage)
- Create: `tools/uitest/Tests/UITestCoreTests/ScriptTests.swift`, `PickTests.swift`
- Modify: `.gitignore` (add `runs/` and `tools/uitest/.build/`)

**Interfaces:**
- Produces:
  ```swift
  public enum Modifier: String, Hashable { case cmd, shift, option, ctrl }
  public struct KeyCombo: Equatable { public let modifiers: Set<Modifier>; public let key: String } // key lowercased: "s", "enter", "escape", "tab", "w"
  public enum Command: Equatable {
    case launch(keepData: Bool)
    case click(label: String, index: Int?, within: String?)   // index is 1-based
    case type(String)
    case key(KeyCombo)
    case expect(String)
    case expectNot(String)
    case wait(seconds: Double)
    case closeWindow
    case handleKeychain
    case quit
  }
  public struct Step: Equatable { public let line: Int; public let source: String; public let command: Command } // source = trimmed line without comment
  public struct ScriptError: Error, Equatable { public let line: Int; public let message: String }
  public func parseScript(_ text: String) throws(ScriptError) -> [Step]
  public enum PickResult<T> { case found(T), none, ambiguous(count: Int) }
  public func pick<T>(_ matches: [T], index: Int?) -> PickResult<T>
  ```
- Grammar: one command per line; blank lines and `#` comments (outside quotes) ignored; strings in double quotes. Script words: `launch [--keep-data]`, `click "X" [index N] [in "Scope"]`, `type "text"`, `key cmd+shift+s`, `expect "X"`, `expect-not "X"`, `wait 30s` / `wait 500ms`, `close-window`, `handle-keychain`, `quit`.

- [ ] **Step 1: Write failing tests** in `ScriptTests.swift`:
  - `parsesAllCommands`: the spec's example script (design doc "Script format") parses to 12 steps; step 2 == `.click(label: "Settings", index: nil, within: nil)`, `wait 30s` == `.wait(seconds: 30)`, `key cmd+s` == `.key(KeyCombo(modifiers: [.cmd], key: "s"))`.
  - `parsesClickScopeAndIndex`: `click "Cancel" in "Discard changes?"` → `within: "Discard changes?"`; `click "Save" index 2` → `index: 2`.
  - `ignoresCommentsAndBlanks`: `# x\n\nquit # bye` → one step, `line == 3`.
  - `hashInsideQuotesIsText`: `type "a # b"` → `.type("a # b")`.
  - `unknownCommandFails`: `frobnicate` throws `ScriptError(line: 1, message: "unknown command: frobnicate")`.
  - `unterminatedQuoteFails`: `click "Save` throws with `line: 1`.
  - `badDurationFails`: `wait soon` throws.
  `PickTests.swift`:
  - `pickSingle`: `pick([7], index: nil)` → `.found(7)`.
  - `pickNone`: `pick([Int](), index: nil)` → `.none`.
  - `pickAmbiguousFails`: `pick([1, 2], index: nil)` → `.ambiguous(count: 2)`.
  - `pickByIndex`: `pick([1, 2], index: 2)` → `.found(2)`; `index: 3` → `.none`.
- [ ] **Step 2: Run** `swift test --package-path tools/uitest` — Expected: FAIL (symbols missing).
- [ ] **Step 3: Implement** `parseScript` (hand-written tokenizer: quoted strings and bare words; no regex library needed) and `pick`.
- [ ] **Step 4: Run** `swift test --package-path tools/uitest` — Expected: all pass.
- [ ] **Step 5: Commit**
  ```bash
  git add .gitignore tools/uitest
  git commit -m "feat(uitest): script parser and label picking" && git push
  ```

### Task 3: Step runner and output formatting

**Files:**
- Create: `tools/uitest/Sources/UITestCore/Runner.swift`
- Create: `tools/uitest/Tests/UITestCoreTests/RunnerTests.swift` (contains a `FakeDriver`)

**Interfaces:**
- Consumes: `Step`, `Command` (Task 2).
- Produces:
  ```swift
  public enum DriverError: Error { case retryable(String), fatal(String) } // retryable = "not there yet"
  public protocol Driver {
    func perform(_ command: Command) throws(DriverError) -> String?   // optional detail appended to ✓ line, e.g. launch's "path (built …)"
    func visibleLabels() -> [String]
    func screenshot(to url: URL) throws
  }
  public struct RunOptions { public var timeout: Double = 10; public var poll: Double = 0.25; public var runDir: URL; public var recordingPath: String; public var secrets: [String] }
  public func runSteps(_ steps: [Step], driver: Driver, options: RunOptions, out: (String) -> Void) -> Bool
  ```
- Behavior: each step retries `perform` on `.retryable` every `poll`s until `timeout`, then fails with `<message> after <timeout>s`; `.fatal` fails immediately; `.wait` is passed to the driver like any other command (driver sleeps). Stops at the first failure. On failure prints screenshot path (`fail-<line>.png` in `runDir`) and `visible:` (first 40 labels, quoted, comma-separated). Last line is the PASS/FAIL summary from Global Constraints. Every output line is passed through redaction: each string in `secrets` replaced with `***`.

- [ ] **Step 1: Write failing tests** in `RunnerTests.swift`:
  - `allPassPrintsChecksAndSummary`: 2 steps, driver succeeds → output `["✓ 1 launch", "✓ 2 quit", "PASS 2/2 steps — recording: r.mp4"]`, returns true.
  - `retryableThenSucceeds`: driver throws `.retryable` twice then succeeds (poll 0.01) → step passes.
  - `retryableTimesOut`: always `.retryable("not found")`, timeout 0.05 → line `✗ 1 expect "X": not found after 0.05s`, screenshot + visible lines follow, summary `FAIL 0/1 steps — recording: r.mp4`, returns false, `screenshot` called with `runDir/fail-1.png`.
  - `fatalStopsImmediately`: `.fatal("ambiguous: 2 matches for \"Cancel\"")` → fails without retrying (driver call count 1) and later steps are not performed.
  - `passwordNeverPrinted`: secrets `["hunter2"]`, driver fatal message contains `hunter2`, visible labels contain `hunter2` → no output line contains `hunter2`.
  - Step number shown is the script **line** number (`Step.line`), matching the spec's `✗ 6`.
- [ ] **Step 2: Run** `swift test --package-path tools/uitest` — Expected: FAIL.
- [ ] **Step 3: Implement** `runSteps` in `Runner.swift`.
- [ ] **Step 4: Run** `swift test --package-path tools/uitest` — Expected: all pass.
- [ ] **Step 5: Commit** `git commit -m "feat(uitest): step runner and output contract" && git push`

### Task 4: macOS driver — AX lookup, mouse, keyboard

**Files:**
- Create: `tools/uitest/Sources/uitest/AXDriver.swift`

**Interfaces:**
- Consumes: `Driver`, `DriverError`, `Command`, `pick` (Tasks 2–3).
- Produces: `final class AXDriver: Driver { init(appPath: URL, keychainPassword: String?) }`. Lifecycle commands (`launch`, `quit`) delegate to `AppLifecycle` (Task 5); until then they throw `.fatal("not implemented")`.
- Matching rule (decided): an element matches label L if `AXTitle`, `AXDescription`, or `AXValue` (string) equals L exactly, after trimming whitespace. `expect`/`expectNot` use **substring** match against the same attributes. Only elements with a non-zero on-screen frame count. `within` restricts the search to descendants of the single element matching the scope label (itself picked with `pick`, index nil).
- Commands: `click` → resolve via `pick`; `.none` → `.retryable("not found")`, `.ambiguous(n)` → `.fatal("ambiguous: n matches for \"L\" — add index N or in \"…\"")`; click = `CGEvent` mouse move, down, up at frame center. `type` → per-character `CGEvent` keyboard events with `keyboardSetUnicodeString`. `key` → virtual key code table for letters, digits, enter, escape, tab, space, delete, arrows + modifier flags. `closeWindow` → click the front window's `kAXCloseButtonAttribute` element (the red traffic light), like a user would. `expectNot` → `.retryable("still visible")` while present. `wait` → `Thread.sleep`. `handleKeychain` → find a window owned by process `SecurityAgent`; absent → return `"no prompt"`; present → click its secure text field, `type` the password, click "Always Allow"; if `keychainPassword` is nil → `.fatal("UITEST_KEYCHAIN_PASSWORD not set")`. (If Task 1 chose stable signing, `handleKeychain` keeps the absent-→"no prompt" branch and the typing branch stays as written; Task 7 makes the prompt not appear.)
- `visibleLabels()` → all non-empty title/description/value strings in the app's AX tree, deduplicated, in tree order. `screenshot(to:)` → `/usr/sbin/screencapture -x <path>`.

- [ ] **Step 1: Implement** `AXDriver` as specified.
- [ ] **Step 2: Manual check** against the running fake-engines app (launched by hand), with a temporary `main.swift` path that attaches to the running app by bundle id `com.app.scribular.tauri`:
  Script: `click "Settings"` / `click "Note styles"` / `expect "Note styles"` / `click "Cancel"` (when two exist).
  Expected: first three `✓`; the last `✗ … ambiguous: 2 matches for "Cancel"`. The cursor visibly moves.
- [ ] **Step 3: Run** `swift test --package-path tools/uitest` — Expected: still all pass.
- [ ] **Step 4: Commit** `git commit -m "feat(uitest): accessibility driver" && git push`

### Task 5: Preflight, app lifecycle, recording, CLI

**Files:**
- Create: `tools/uitest/Sources/UITestCore/DataDir.swift`
- Create: `tools/uitest/Sources/uitest/AppLifecycle.swift`, `Recorder.swift`, `Preflight.swift`
- Modify: `tools/uitest/Sources/uitest/main.swift`, `AXDriver.swift` (wire `launch`/`quit`)
- Test: `tools/uitest/Tests/UITestCoreTests/DataDirTests.swift`

**Interfaces:**
- Produces:
  ```swift
  // UITestCore (pure, testable)
  public enum DataDirMove: Equatable { case moved(to: URL), absent }
  public func moveDataDir(from: URL, intoRunDir: URL) throws -> DataDirMove  // destination: <runDir>/app-data; throws if destination exists
  ```
- CLI: `uitest <script> [--app <path to .app>]`. Default app: `src-tauri/target/release/bundle/macos/Scribular.app` relative to cwd. Creates `runs/<ts>/`, runs preflight, starts recorder, parses + runs steps, then **always** (normal exit, failure, SIGINT/SIGTERM) quits the app and stops the recorder. Exit codes per Global Constraints. Parse errors print `parse error line N: <message>` and exit 2 before anything launches.
- Preflight (exit 2 with one-line fix each): `AXIsProcessTrusted()` false → "grant Accessibility to <host process> in System Settings → Privacy & Security → Accessibility"; `CGPreflightScreenCaptureAccess()` false → same for Screen Recording; `ffmpeg` not on PATH; app path missing → "build first: <build command from Task 1 findings>"; display asleep/locked (`CGSessionCopyCurrentDictionary` `CGSSessionScreenIsLocked`) → "unlock the display".
- `launch`: terminate any running `com.app.scribular.tauri` instance (wait up to 5 s); unless `keepData`, `moveDataDir`; `open -n <app>` equivalent via `NSWorkspace.openApplication`; wait (retryable) until the app has an AX window. Detail string: `<app path> (built <exe mtime ISO8601>)`.
- `quit`: `NSRunningApplication.terminate()`, then `forceTerminate()` after 5 s.
- Recorder: spawns `ffmpeg -f avfoundation -framerate 15 -capture_cursor 1 -i "<screen>:none" -pix_fmt yuv420p -movflags +frag_keyframe+empty_moov <runDir>/recording.mp4` (screen index resolved at runtime by matching `Capture screen` in `ffmpeg -f avfoundation -list_devices true -i ""`; ffmpeg stderr logged to `<runDir>/ffmpeg.log`; fragmented MP4 keeps the file playable after a hard crash); stop by writing `q` to its stdin and waiting up to 10 s.

- [ ] **Step 1: Write failing tests** in `DataDirTests.swift` (temp dirs via `FileManager.default.temporaryDirectory`):
  - `moveDataDirMovesContents`: source with file `a.db` → `.moved(to: run/app-data)`, `run/app-data/a.db` exists, source gone.
  - `moveDataDirAbsentIsNoop`: missing source → `.absent`, no throw.
  - `moveDataDirRefusesOverwrite`: `run/app-data` already exists → throws; source untouched.
- [ ] **Step 2: Run** `swift test --package-path tools/uitest` — Expected: FAIL.
- [ ] **Step 3: Implement** `moveDataDir`, then `Preflight`, `AppLifecycle`, `Recorder`, `main.swift` as specified.
- [ ] **Step 4: Run** `swift test --package-path tools/uitest` — Expected: all pass.
- [ ] **Step 5: Manual checks** with a 3-line script (`launch` / `expect "Settings"` / `quit`):
  - Run `swift run --package-path tools/uitest uitest smoke.uitest --app <p03 app>` → `PASS 3/3`, mp4 plays, `runs/<ts>/app-data` exists.
  - Run again while the app is already open → still `PASS` (old instance terminated).
  - Add `wait 30s`, press Ctrl-C mid-wait → app quits, mp4 still plays.
  - Revoke nothing; temporarily pass `--app /nope.app` → exit 2 with the build hint.
- [ ] **Step 6: Commit** `git commit -m "feat(uitest): preflight, launch, recording, CLI" && git push`

### Task 6: Success scenarios and README

**Files:**
- Create: `tools/uitest/scenarios/as11-note-style-default.uitest`, `as13-dirty-close-cancel.uitest`, `keychain-after-rebuild.uitest`, `as12-regenerate-picker.uitest`
- Create: `tools/uitest/README.md`

**Content decided:**
- AS11: launch → Settings → Note styles → new style, name `UITest Recap`, prompt text `Bullet points only.`, save, `click "Toggle favourite for UITest Recap"`, make default, back to recording view, start recording, `wait 30s`, stop, open the new session, `expect "UITest Recap"` (provenance), quit. Exact button labels come from Task 1's AX dump.
- AS13: launch → Note styles → open a style, type into its prompt (dirty), `close-window`, `expect "Discard changes?"`, `click "Cancel" in "Discard changes?"`, `expect-not "Discard changes?"`, `expect "Note styles"` (window still open), quit.
- Keychain: `launch --keep-data`, `handle-keychain`, `expect "Settings"`, quit. Run right after a rebuild.
- AS12 UI: written against spec 09's picker labels once it lands; until then the file has a header comment `# blocked on spec 09` and is not part of the success check.
- README: one-time setup (grant Accessibility + Screen Recording to the terminal/agent host, `brew install ffmpeg`, export `UITEST_KEYCHAIN_PASSWORD`), build command (from Task 1), run command, the script grammar table (one row per command), the output contract, where recordings land, and the stale-build warning.

- [ ] **Step 1: Write the scenarios and README.**
- [ ] **Step 2: Rebuild the p03 app**, then run `keychain-after-rebuild.uitest` — Expected: `PASS` (prompt handled, or `no prompt` if Task 7 is in place).
- [ ] **Step 3: Run** `as11-note-style-default.uitest` and `as13-dirty-close-cancel.uitest` — Expected: both `PASS`; open each `recording.mp4` and confirm it shows the flow.
- [ ] **Step 4: Break one on purpose:** change AS11's `expect` text to `Nope` → Expected: `✗` line, screenshot path, `visible:` list including `UITest Recap`. Revert.
- [ ] **Step 5: Commit** `git commit -m "feat(uitest): PR #10 acceptance scenarios and README" && git push`

### Task 7 (only if Task 1 found keychain typing blocked): Stable local signing

**Files:**
- Modify: `../scribular-tauri-p03/src-tauri/tauri.conf.json` — `bundle.macOS.signingIdentity` from env (`APPLE_SIGNING_IDENTITY`; Tauri reads it), not hard-coded, so CI signing (`plans/2026-11-03-u11-mac-signing-github-actions-plan.md`) is unaffected.
- Modify: `tools/uitest/README.md` (cert setup section)

- [ ] **Step 1: Create** a self-signed code-signing certificate `Scribular Local Dev` in the login keychain (Keychain Access → Certificate Assistant → Create a Certificate, type Code Signing). Document the exact clicks in the README.
- [ ] **Step 2: Build** with `APPLE_SIGNING_IDENTITY="Scribular Local Dev"` + the Task 1 build command. Verify `codesign -dv <app> 2>&1 | grep Authority` → `Authority=Scribular Local Dev`.
- [ ] **Step 3: Launch once by hand**, answer the keychain prompt with "Always Allow". Rebuild (touch a source file), launch again → Expected: no prompt.
- [ ] **Step 4: Run** `keychain-after-rebuild.uitest` → Expected: `✓ 2 handle-keychain no prompt`, `PASS`.
- [ ] **Step 5: Commit** (README in this repo; p03 change in p03 on its branch), `git push` both.
