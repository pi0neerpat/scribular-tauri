# Native UI test runner — design

Status: approved 2026-11-05. Next step: implementation plan.

## Why

The review agent verifies Rust and Vitest behavior but cannot drive the real Tauri app. In the PR #10 product review ("Product review: Note styles and summary model choice"), AS11, AS12 UI and AS13 were left as "Needs Patrick: unverified manual checks" because "Native app UI could not be controlled." Every PR with UI acceptance criteria hits the same wall.

Tauri's official WebDriver path (`tauri-driver`) does not work on macOS: WKWebView has no WebDriver. The Electron predecessor (`../notes-assistant-electron/test/`, WebdriverIO + `--e2e-test` fake backend) relied on Chromium and cannot be ported directly. What carries over is the idea of a fake backend flag (here: the `fake-engines` Cargo feature) and small reusable helpers.

## What it is

A small command-line tool that reads a plain-text script of user actions and performs them against the real running app with real OS-level mouse and keyboard input, like a person would. It records the screen the whole time.

- **The agent** writes the script, runs the tool, and reads back a compact per-step result. On failure it also gets a screenshot and the list of labels currently visible, so it can correct the script.
- **The human reviewer** gets a screen recording saved locally on the machine that ran the test. Not uploaded, not committed.

## Success criteria

The review agent, on this VM, can run these scenarios from PR #10 itself and mark them verified, each with a recording Patrick can watch:

- **AS11** — Settings → Note styles; create a style, enter a prompt, save, star it, make it default; record ~30 s and stop; confirm the session shows the style's name (summary content is fake under `fake-engines`, so confirm provenance, not prose quality).
- **AS12 UI** — in a finished session, open the regenerate picker; confirm it lists favourites and the session's style; choose built-in Narrative and regenerate; confirm three labelled sections; confirm the default style is unchanged. (Depends on spec 09 landing the picker; the tool must support it, the scenario runs once the UI exists.)
- **AS13** — with a dirty style editor, close the window; confirm the discard prompt appears; choose Cancel; confirm the window stays open.
- **Keychain** — after a rebuild, the macOS keychain prompt appears and the script gets past it (see below).

AS20/AS21 (second model download) are out of scope: the model doesn't exist yet.

## Decisions made (Patrick)

- Script-driven: the agent writes a simple action script; the tool interprets it against the real app.
- Real mouse + keyboard simulation, not DOM injection.
- Agent gets back relevant text output, plus a screenshot when things go wrong.
- Human gets a screen recording viewable locally; nothing goes to GitHub or git.
- Runs on this VM (a dedicated machine; taking over the cursor is fine).
- **Keychain prompt: the script handles it.** It types the VM login password from an environment variable and clicks "Always Allow." (Chosen over stable code signing and over a test-only file keystore.)
- Keep it simple.

## Calls made by Claude (correct if wrong)

- **Language: one Swift command-line tool.** Swift is already on the VM and reaches the macOS APIs directly with no packages: Accessibility API (find elements by label), `CGEvent` (mouse/keyboard), `screencapture` for failure screenshots. No new dependencies.
- **Recording: `ffmpeg`** (already installed) capturing the screen via `avfoundation` from first step to last, written to `runs/<timestamp>/recording.mp4`. `runs/` is git-ignored.
- **Elements are targeted by visible label**, not DOM ids. WKWebView exposes web buttons and text to the Accessibility tree, so one lookup covers the Svelte UI, native dialogs, and the keychain prompt. Scripts stay readable for humans.
- **App runs with `fake-engines`** so recording/summarizing is fast and deterministic. A real-model run is a possible later addition, not part of this.
- **Location:** `tools/uitest/` in this repo, with example scripts for the success scenarios in `tools/uitest/scenarios/`.

## Settled after builder review (Claude's calls)

- **First task is a spike, not the tool.** Before building the runner, prove three things by hand on this VM with a throwaway Swift snippet: (a) AX can see the Note styles screen's controls (star, default, save, the picker) by label; (b) AX + `CGEvent` can find and act on the keychain prompt (see risk below); (c) `ffmpeg` can record the screen. Any unlabeled control gets an `aria-label` in the app: that counts as an allowed small fix.
- **Preflight check.** The tool starts by checking Accessibility and Screen Recording permission for its host process, plus an awake, unlocked display. If either is missing it exits with a one-line fix instead of failing mid-script. Setup steps get documented in `tools/uitest/README.md`.
- **Build is separate from launch.** `launch` only runs an already-built `fake-engines` app and prints its path + build time. The README gives the build command. No auto-build.
- **Clean state per run.** `launch` moves the app's data dir (`~/Library/Application Support/com.app.scribular.tauri`) into the run folder before starting, so each run starts fresh and nothing is deleted. Opt out with `launch --keep-data`.
- **AS13's discard prompt is in-app (Svelte), not native.** `NoteStyles.svelte:49` intercepts `onCloseRequested` and shows its own dialog, so AX sees it inside the webview. The "Cancel" ambiguity is handled by the builder's disambiguation rule (e.g. scope to the dialog).
- **Recording on early failure:** stop ffmpeg cleanly (send `q`) on any exit path so the mp4 is always playable.

## Known risk: keychain prompt

The keychain prompt is drawn by SecurityAgent, which uses secure input. On recent macOS, simulated keystrokes into its password field may be blocked. The spike checks this first. Fallback if blocked (Patrick's decision): **stable signing.** Sign every build with one fixed local self-signed certificate (`bundle.macOS.signingIdentity`), so the keychain trusts the app across rebuilds and the prompt appears once, then never again. `handle-keychain` stays as a no-op-if-absent step.

## Script format (sketch, details to the builder)

One action per line, `#` comments, quoted strings:

```
launch                      # start the built app, wait for its window
click "Settings"
click "Note styles"
click "New style"
type "Bullet recap"
key cmd+s
expect "Bullet recap"       # text/label visible within timeout
expect-not "Discard changes?"
wait 30s
close-window
handle-keychain             # if the keychain prompt is up: type $UITEST_KEYCHAIN_PASSWORD, click "Always Allow"
quit
```

Minimum command set: whatever AS11, AS12 UI, AS13 and the keychain step need: launch, click, type, key, expect, expect-not, wait, close-window, handle-keychain, quit. Click needs a way to disambiguate duplicate labels (e.g. the Nth match, or role such as button). The builder decides how.

## Output contract

Stdout, one line per step, then a summary, so the agent can parse it without extras:

```
✓ 1 launch
✓ 2 click "Settings"
✗ 6 expect "Bullet recap": not found after 10s
  screenshot: runs/2026-11-05-1312/fail-6.png
  visible: "Settings", "Note styles", "New style", "Save", "Cancel", ...
FAIL 5/6 steps — recording: runs/2026-11-05-1312/recording.mp4
```

Exit code 0 on pass, non-zero on fail. The recording path is always printed.

## Constraints

- macOS only (this VM: macOS 26.2, Apple Silicon). The terminal/agent process needs Accessibility and Screen Recording permissions; granting them is a one-time manual setup step to document.
- Keychain password only from an env var (`UITEST_KEYCHAIN_PASSWORD`), never in scripts, logs or recordings' text output.
- Tests run against a built app. The tool must say which binary it launched (path + build time) to avoid the Electron repo's "stale build" trap (`notes-assistant-electron/docs/TESTING.md`).

## Non-goals

- CI integration, other OSes, parallel runs.
- A test framework, assertion library, or reporting beyond the output above.
- Uploading recordings anywhere.
- Pixel/visual diffing.
- Verifying AI summary quality.

## Anti-goals (would be a failure even if it technically works)

- Scripts the agent can't write reliably without trial and error on every run (labels must be discoverable via the failure output).
- Flaky passes: a step must fail loudly rather than click the wrong thing.
- Requiring app code changes just to make the tool work. Small accessibility-label fixes in the UI are fine if a control truly has no label.

## Left to the builder

- Exact script grammar, timeouts, and how to disambiguate duplicate labels.
- How `launch` finds/starts the app (built `.app` vs `tauri dev`), as long as the binary is reported.
- How `expect` searches (exact vs substring match).
- Recording details (frame rate, which display, stopping ffmpeg cleanly).

## Spike findings

Partial check on 2026-11-05:

- Build command verified in `../scribular-tauri-p03`: `npm run tauri -- build --features fake-engines --bundles app`. It produced `/Users/admin/cargo-target/scribular-tauri/release/bundle/macos/Scribular.app` on this machine.
- `ffmpeg -f avfoundation -list_devices true -i ""` listed no video or audio devices in this session. `Capture screen 0` and recording playback remain unverified; the runner uses `0:none` provisionally.
- Accessibility tree inspection and SecurityAgent keychain typing were not run because this session had no desktop-control tool. Keychain typing remains unverified; do not switch to stable signing without that check.
- No unlabeled controls were identified, and no app accessibility labels were changed.
