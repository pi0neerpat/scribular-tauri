# Native UI test runner

`uitest` drives the built macOS app with Accessibility and real mouse/keyboard events. Screen recording is opt-in; without `--record`, no ffmpeg recorder starts. Failed steps still save a screenshot in `runs/<timestamp>/`. Nothing is uploaded.

## One-time setup

1. Grant Accessibility and Screen Recording access to the terminal or agent host that runs `uitest` in System Settings → Privacy & Security. Restart that host after changing permissions.
2. Install `ffmpeg` with `brew install ffmpeg`.
3. Create the local signing identity once with `bash tools/uitest/setup-signing.sh`. It adds a self-signed "Scribular Local Dev" codesigning certificate to the login keychain (nothing is written to the repo). Apps signed with it keep the same identity across rebuilds, so the Screen & System Audio Recording grant and the keychain ACL survive a rebuild: you answer each prompt once, by hand, on the first signed launch.
4. Store the Mac login password once with `bash tools/uitest/trust-build.sh --store-password`. Every `launch` then runs `trust-build.sh`, which uses it to trust each new build (see below). It is saved in the login keychain (service `scribular-uitest`), readable by `/usr/bin/security` without a prompt, so any process running as you can read it. Re-run `--store-password` after changing your Mac login password; a wrong stored password is reported on the next launch. Remove it with `security delete-generic-password -s scribular-uitest -a login-password`.

   `UITEST_KEYCHAIN_PASSWORD`, if set, overrides the stored password and lets `handle-keychain` answer any prompt that still appears. The runner redacts it from output.

### Run safety and failure evidence

Each run takes a non-blocking lock at `~/Library/Caches/scribular-uitest/uitest.lock`; a second run exits with code 2 and reports the active PID. The lock is held until the process exits. Before a mouse click, the runner waits up to 2 seconds for Scribular to become frontmost and checks the click point with the system Accessibility hit-test. For `click` steps only, if the point is covered or Scribular is not frontmost, the runner presses the resolved target through Accessibility; the success log marks this as `pressed via Accessibility (...)`. This was verified with a window covering Stop in AS2 (`runs/as2-covered-stop-20261109T104107/uitest.log`, 15/15 steps passed). The guard also checks that a same-process hit belongs to the resolved target, so another Scribular window is treated as a cover. An open keychain prompt remains a retryable failure. Other covered-click retries continue until the step timeout.

Failed steps save the existing cropped `fail-<line>.png` plus a full-display `fail-<line>-screen.png`, and print the frontmost app name. After quitting the app, every run copies a snapshot of `scribular.log` to `app.log` in that run directory when the source log exists. With `launch --keep-data`, that snapshot can include earlier app runs. A missing source or copy error is printed in the run output.

### Scenario rules

- Before a run, check `pgrep -fl "uitest|Scribular.app|codex"`, stop any other uitest or Scribular process, make sure no other window covers Scribular, and trust the exact app build with `bash tools/uitest/trust-build.sh /path/to/Scribular.app`.
- Use `play "fixture.wav" loop` only to make `afplay` available in the App picker. Stop or supersede it with a one-shot `play "fixture.wav"` immediately before each `Start recording` action.
- Do not assert live transcript phrases while recording. After Stop, assert only phrases stable across Whisper runs (for example, `since we last met`).
- On a slow VM, allow at least 400 seconds for the summary.
- Never run two uitest runs or other GUI automation at once.

## Build and run

Build the signed app separately; `launch` never builds. `launch` runs `trust-build.sh` first: a self-signed cert has no Team ID, so macOS gates keychain items by the build's cdhash (their partition list), which changes each build. The script adds the new cdhash to Scribular's keychain items, so the rebuilt app reads its key without a prompt. `launch` fails with a clear message if no password is stored.

```sh
APPLE_SIGNING_IDENTITY="Scribular Local Dev" TAURI_SIGNING_PRIVATE_KEY="<throwaway key>" TAURI_SIGNING_PRIVATE_KEY_PASSWORD="<password if set>" npm run tauri -- build --bundles app
swift run --package-path tools/uitest uitest tools/uitest/scenarios/as11-note-style-default.uitest --app /path/to/Scribular.app
swift run --package-path tools/uitest uitest tools/uitest/scenarios/system-audio-smoke.uitest --record 04-system-audio-happy-path --app /path/to/Scribular.app
```

Use `--record <name>` only for a happy path or an issue that cannot be reproduced reliably. The recorder uses libx264's ultrafast preset to keep CPU free for the app (files are larger). Use names like `04-system-audio-happy-path`; recordings are written as `<record-dir>/<name>.mp4` with a matching JSON metadata file. Failed recorded runs use `<name>-FAILED.mp4`. `--record-dir <dir>` overrides `CWF_RECORDINGS_DIR`; otherwise a clone with a sibling `scribular-tauri` checkout writes there, falling back to `./runs/`. Recordings are not committed.

Recorded runs default to 2.5 seconds of pacing after UI actions. `--pace 2s` overrides it (`--pace 0` disables it); unrecorded runs default to no pacing. In scripts, `pause 3s` pauses for reading and `pace 2s` changes the action pace from that point. An `expect` or `expect-not` after an action waits one extra second for the new state to be read.

Generate a local throwaway updater key with `npm run tauri signer generate -- -w <path>` and use its private key value for `TAURI_SIGNING_PRIVATE_KEY`; set `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` only if the key has a password.

This machine places the bundle at `/Users/admin/cargo-target/scribular-tauri/release/bundle/macos/Scribular.app`; pass the path printed by your own build with `--app` if it differs. `launch` reports the app path and executable modification time so stale builds are visible.

Other scenarios:

```sh
swift run --package-path tools/uitest uitest tools/uitest/scenarios/as13-dirty-close-cancel.uitest --app /path/to/Scribular.app
swift run --package-path tools/uitest uitest tools/uitest/scenarios/keychain-after-rebuild.uitest --app /path/to/Scribular.app
```

### Model cache

`launch` points the app's `models` folder (`~/Library/Application Support/com.app.scribular.tauri/models`) at a symlink to `~/Library/Caches/scribular-uitest/models`. The first run downloads about 1.85 GB of models into it; later runs reuse it and skip the download. If app data already holds a real `models` folder from a manual run, `launch` moves its files into the cache instead of into the run folder. Files already in the cache are never overwritten or deleted; the app verifies them itself.

Clear it with `rm -rf ~/Library/Caches/scribular-uitest` only to free disk space. Do not clear it to "fix" a failing test: a missing model means a 1.85 GB re-download, not a different result.

### Recording tests

Use a recording only for a happy path or an issue that cannot be reproduced reliably. Record a fixture file through App audio, never the microphone. `system-audio-smoke.uitest` is the template:

1. `play "fixture.wav" loop` so `afplay` is playing and appears in the App picker.
2. In Settings choose `App audio`, then run `pick-app "afplay" timeout 60s`. Each attempt closes any open menu, refreshes the list, waits 2.5 seconds, and opens the App picker. It waits for the menu to open and close, detected through the window server, types `afplay`, presses Return, waits 1 second, and verifies the picker's value; a mismatch is retried. The native menu does not update while open, and its items are not exposed to Accessibility, so selection cannot rely on reading its menu items. The 1-second wait accounts for the first click after the menu closes being swallowed.
3. Go to Record and run `play "fixture.wav"` once more without `loop`, immediately before `Start recording`. Capture follows the `afplay` process running at Start, and each loop restart is a new process, so recording a loop captures only the rest of the current pass.
4. Wait a little longer than the clip, then click `Stop` and run `stop-audio`.
5. `expect` a phrase that appeared identically in several recordings of the fixture. Do not assert summary wording, which varies; `expect "Summary·"` only checks that a summary exists. On an 8 GB VM, allow `expect "Summary·" timeout 400s` or longer.

The runner uses AVFoundation's `Capture screen 0` device (`0:none`). Check that device with `ffmpeg -f avfoundation -list_devices true -i ""` if screen capture does not start. Keychain typing and the Accessibility labels still need verification in a live desktop session after a rebuild.

## Script commands

One command per line. Blank lines and `#` comments are ignored; text values use double quotes.

| Command | Meaning |
| --- | --- |
| `launch [--keep-data]` | Start the built app. By default, move its app data into the run folder first. |
| `click "Label" [index N] [in "Scope"]` | Click the unique exact label; use a 1-based index or scope to disambiguate. |
| `pick-app "afplay" timeout 60s` | Close any menu left open, refresh the App-audio list, wait 2.5 seconds, open the App picker, and wait for the menu to open and close as detected by the window server. Type-select the name, press Return, wait 1 second, verify the picker value, and retry if it differs. |
| `type "text"` | Type into the focused control. |
| `key cmd+shift+s` | Press a key with optional `cmd`, `shift`, `option`, or `ctrl` modifiers. |
| `expect "text"` | Wait until visible text contains the value. |
| `expect-not "text"` | Wait until visible text no longer contains the value. |
| `wait 30s` / `wait 500ms` | Pause for a duration. |
| `pause 3s` | Pause for reading without changing the configured action pace. |
| `pace 2s` | Change the delay after UI actions for the rest of the script. |
| `<step> timeout 300s` | End a `launch`, `click`, `pick-app`, `expect`, `expect-not` or `close-window` step with this to retry it for that long instead of the default 10s (e.g. while a model downloads). `pick-app` reports the last failure and attempt count if it times out. |
| `close-window` | Click the front window's close control. |
| `handle-keychain` | If SecurityAgent has a prompt, type the environment password and choose Always Allow. |
| `labels` | Print a count, then every accessible label with its role (`Button "Save"`), one line per element, so you can write `click`/`expect` lines without reading the app source. Button, link, menu and similar actionable labels are printed in full; values and other text are cut at 80 characters; repeated elements are listed separately (use `click ... index N`). Off-screen web content may be included. |
| `screenshot ["name"]` | Save the app window to `runs/<timestamp>/<name>.png` (default `step-<line>.png`) for checking layout. The step prints the path. |
| `play "file.wav" [loop]` | Play the file through `/usr/bin/afplay` in the background, so the app can record it in App-audio mode (`afplay` in the App picker). Relative paths resolve against the directory uitest runs from; a missing file fails the step. `loop` restarts it when it ends. Stops at `stop-audio`, `quit`, or the end of the run (pass or fail). |
| `stop-audio` | Stop `play` playback. |
| `quit` | Quit the app and stop `play` playback. |
| `expect-status-item "Scribular"` | Wait for the app status item whose Accessibility help equals the text. |
| `status-item click "Scribular"` | Press the matching app status item. Native menu items are not exposed to Accessibility. |
| `screenshot-menubar ["name"]` | Capture the first 40 points of the main display to `menubar-<line>.png` or the named file. |
| `screenshot-screen ["name"]` | Capture the whole main display to `screen-<line>.png` or the named file. |
| `expect-notification "text"` | Wait until a Notification Center banner's text contains the value. |
| `dock click "Scribular"` | Press the unique matching Dock item. |

`timeout 30s` also applies to `expect-status-item` and `expect-notification`. Notifications are brief (about 3–4 seconds in the tested VM), so put `expect-notification` directly after the action that triggers the banner. It searches banner text in Notification Center; it does not read notification history.

### Native surfaces and fallbacks

Use the native UI command first, then a surface screenshot, then a log or state assertion, then simulated evidence, then mark the scenario unverified. The Scribular status item is reachable with `expect-status-item` and `status-item click`. The opened native menu's items are not exposed to Accessibility; inspect them only with `screenshot-screen`.

`screenshot-menubar` captures the main display's top 40 points. `screenshot-screen` captures the whole main display and is useful for the status menu or notification banner. These commands save to the current run directory and print the path; existing `screenshot` behavior is unchanged.

### Doctor

Run `swift run --package-path tools/uitest uitest doctor [--app /path/to/Scribular.app]` to check host permissions, System Events automation, `ffmpeg`, the stored keychain password, and optionally the app bundle. It prints every check as `ok` or `MISSING`, includes the fix command, exits 1 if any check is missing, and never prints the stored password. With `--app`, it also prints the per-build trust command; the current cdhash trust cannot be detected automatically.

### Keychain after rebuild

Store the login password once with `bash tools/uitest/trust-build.sh --store-password`. After each rebuild, run `bash tools/uitest/trust-build.sh <path>/Scribular.app` so the new build's cdhash can read the existing Scribular keychain items. Keep `handle-keychain` in a scenario as a safety net for a prompt that still appears; the Always-Allow path is not verified.

### Developer controls

The Developer controls require a debug build because both the frontend section and native test commands are compile-time gated. From the repository root:

```sh
CARGO_TARGET_DIR=/Users/admin/cargo-target/<clone> APPLE_SIGNING_IDENTITY='Scribular Local Dev' npm run tauri -- build --debug --bundles app --config tools/uitest/tauri-debug-overlay.json
bash tools/uitest/trust-build.sh /Users/admin/cargo-target/<clone>/debug/bundle/macos/Scribular.app
```

The release bundle must continue to show `Privacy & diagnostics` and no `Developer` section.

To explore a screen, write a short script that navigates there and ends with `labels` and `screenshot`. Put an `expect` before `labels`: right after `launch` the web content is not loaded yet, and `labels` lists only the window and menu bar.

```text
launch
click "Settings"
expect "Note styles"
labels
screenshot "settings"
quit
```

## Output

Each action prints one line (`labels` prints a count line plus one line per label). A failed step stops the run and prints its cropped screenshot, full-display screenshot, frontmost app, and up to 40 visible labels. Each run directory also gets `app.log` after quit when the app log exists. A successful run's result line includes the recording path (or `no recording`). Failed runs print their failure summary, then print the final recording path (`-FAILED.mp4` for a recorded run, or `no recording`):

```text
✓ 1 launch /path/to/Scribular.app (built 2026-11-05T20:00:00Z)
✗ 6 expect "Bullet recap": not found after 10s
  screenshot: runs/<timestamp>/fail-6.png
  screen: runs/<timestamp>/fail-6-screen.png
  frontmost: Scribular
  visible: "Settings", "Note styles", ...
FAIL 5/6 steps — recording: no recording
```

Exit codes: `0` for pass, `1` for a failed step, and `2` for a parse or preflight error. `as12-regenerate-picker.uitest` is a placeholder until spec 09 lands. Scenario labels were taken from the app source; the manual Accessibility-tree and screen-recording checks still need to be run on a desktop session.
