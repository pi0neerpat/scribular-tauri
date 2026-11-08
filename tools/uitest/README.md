# Native UI test runner

`uitest` drives the built macOS app with Accessibility and real mouse/keyboard events. Screen recording is opt-in; without `--record`, no ffmpeg recorder starts. Failed steps still save a screenshot in `runs/<timestamp>/`. Nothing is uploaded.

## One-time setup

1. Grant Accessibility and Screen Recording access to the terminal or agent host that runs `uitest` in System Settings → Privacy & Security. Restart that host after changing permissions.
2. Install `ffmpeg` with `brew install ffmpeg`.
3. Create the local signing identity once with `bash tools/uitest/setup-signing.sh`. It adds a self-signed "Scribular Local Dev" codesigning certificate to the login keychain (nothing is written to the repo). Apps signed with it keep the same identity across rebuilds, so the Screen & System Audio Recording grant and the keychain ACL survive a rebuild: you answer each prompt once, by hand, on the first signed launch.
4. Store the Mac login password once with `bash tools/uitest/trust-build.sh --store-password`. Every `launch` then runs `trust-build.sh`, which uses it to trust each new build (see below). It is saved in the login keychain (service `scribular-uitest`), readable by `/usr/bin/security` without a prompt, so any process running as you can read it. Re-run `--store-password` after changing your Mac login password; a wrong stored password is reported on the next launch. Remove it with `security delete-generic-password -s scribular-uitest -a login-password`.

   `UITEST_KEYCHAIN_PASSWORD`, if set, overrides the stored password and lets `handle-keychain` answer any prompt that still appears. The runner redacts it from output.

## Build and run

Build the signed app separately; `launch` never builds. `launch` runs `trust-build.sh` first: a self-signed cert has no Team ID, so macOS gates keychain items by the build's cdhash (their partition list), which changes each build. The script adds the new cdhash to Scribular's keychain items, so the rebuilt app reads its key without a prompt. `launch` fails with a clear message if no password is stored.

```sh
APPLE_SIGNING_IDENTITY="Scribular Local Dev" TAURI_SIGNING_PRIVATE_KEY="<throwaway key>" TAURI_SIGNING_PRIVATE_KEY_PASSWORD="<password if set>" npm run tauri -- build --bundles app
swift run --package-path tools/uitest uitest tools/uitest/scenarios/as11-note-style-default.uitest --app /path/to/Scribular.app
swift run --package-path tools/uitest uitest tools/uitest/scenarios/system-audio-smoke.uitest --record 04-system-audio-happy-path --app /path/to/Scribular.app
```

Use `--record <name>` only for a happy path or an issue that cannot be reproduced reliably. Use names like `04-system-audio-happy-path`; recordings are written as `<record-dir>/<name>.mp4` with a matching JSON metadata file. Failed recorded runs use `<name>-FAILED.mp4`. `--record-dir <dir>` overrides `CWF_RECORDINGS_DIR`; otherwise a clone with a sibling `scribular-tauri` checkout writes there, falling back to `./runs/`. Recordings are not committed.

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
2. In Settings choose `App audio`, click `Refresh`, `wait 6s`, then open `Choose an app…` and click `afplay`. The open menu does not update, so the list must load first. Add `wait 1s` after picking, because the first click after the menu closes is swallowed.
3. Go to Record and run `play "fixture.wav"` once more without `loop`, right before `Start recording`. Capture follows the `afplay` process running at Start, and each loop restart is a new process, so recording a loop captures only the rest of the current pass.
4. Wait a little longer than the clip, then click `Stop` and run `stop-audio`.
5. `expect` a phrase that appeared identically in several recordings of the fixture. Do not assert summary wording, which varies; `expect "Summary·"` only checks that a summary exists.

The runner uses AVFoundation's `Capture screen 0` device (`0:none`). Check that device with `ffmpeg -f avfoundation -list_devices true -i ""` if screen capture does not start. Keychain typing and the Accessibility labels still need verification in a live desktop session after a rebuild.

## Script commands

One command per line. Blank lines and `#` comments are ignored; text values use double quotes.

| Command | Meaning |
| --- | --- |
| `launch [--keep-data]` | Start the built app. By default, move its app data into the run folder first. |
| `click "Label" [index N] [in "Scope"]` | Click the unique exact label; use a 1-based index or scope to disambiguate. |
| `type "text"` | Type into the focused control. |
| `key cmd+shift+s` | Press a key with optional `cmd`, `shift`, `option`, or `ctrl` modifiers. |
| `expect "text"` | Wait until visible text contains the value. |
| `expect-not "text"` | Wait until visible text no longer contains the value. |
| `wait 30s` / `wait 500ms` | Pause for a duration. |
| `pause 3s` | Pause for reading without changing the configured action pace. |
| `pace 2s` | Change the delay after UI actions for the rest of the script. |
| `<step> timeout 300s` | End a `launch`, `click`, `expect`, `expect-not` or `close-window` step with this to retry it for that long instead of the default 10s (e.g. while a model downloads). |
| `close-window` | Click the front window's close control. |
| `handle-keychain` | If SecurityAgent has a prompt, type the environment password and choose Always Allow. |
| `labels` | Print a count, then every accessible label with its role (`Button "Save"`), one line per element, so you can write `click`/`expect` lines without reading the app source. Button, link, menu and similar actionable labels are printed in full; values and other text are cut at 80 characters; repeated elements are listed separately (use `click ... index N`). Off-screen web content may be included. |
| `screenshot ["name"]` | Save the app window to `runs/<timestamp>/<name>.png` (default `step-<line>.png`) for checking layout. The step prints the path. |
| `play "file.wav" [loop]` | Play the file through `/usr/bin/afplay` in the background, so the app can record it in App-audio mode (`afplay` in the App picker). Relative paths resolve against the directory uitest runs from; a missing file fails the step. `loop` restarts it when it ends. Stops at `stop-audio`, `quit`, or the end of the run (pass or fail). |
| `stop-audio` | Stop `play` playback. |
| `quit` | Quit the app and stop `play` playback. |

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

Each action prints one line (`labels` prints a count line plus one line per label). A failed step stops the run and prints its screenshot path and up to 40 visible labels. Screenshots are cropped to the app window, or the full screen when a keychain prompt is open or no window exists. A successful run's result line includes the recording path (or `no recording`). Failed runs print their failure summary, then print the final recording path (`-FAILED.mp4` for a recorded run):

```text
✓ 1 launch /path/to/Scribular.app (built 2026-11-05T20:00:00Z)
✗ 6 expect "Bullet recap": not found after 10s
  screenshot: runs/<timestamp>/fail-6.png
  visible: "Settings", "Note styles", ...
FAIL 5/6 steps — recording: no recording
```

Exit codes: `0` for pass, `1` for a failed step, and `2` for a parse or preflight error. `as12-regenerate-picker.uitest` is a placeholder until spec 09 lands. Scenario labels were taken from the app source; the manual Accessibility-tree and screen-recording checks still need to be run on a desktop session.
