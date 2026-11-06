# Native UI test runner

`uitest` drives the built macOS app with Accessibility and real mouse/keyboard events. Each run records the screen locally in `runs/<timestamp>/recording.mp4`; failed steps also save a screenshot there. Nothing is uploaded.

## One-time setup

1. Grant Accessibility and Screen Recording access to the terminal or agent host that runs `uitest` in System Settings → Privacy & Security. Restart that host after changing permissions.
2. Install `ffmpeg` with `brew install ffmpeg`.
3. For keychain prompts, export the Mac login password in the environment only:

   ```sh
   export UITEST_KEYCHAIN_PASSWORD='…'
   ```

   The runner redacts this value from output. It is never read from a script.

## Build and run

Build the fake-engine app separately; `launch` never builds. From the parity checkout:

```sh
cd ../scribular-tauri-p03
npm run tauri -- build --features fake-engines --bundles app
cd ../scribular-tauri
swift run --package-path tools/uitest uitest tools/uitest/scenarios/as11-note-style-default.uitest --app /path/to/Scribular.app
```

The build was verified with that command. This machine placed the bundle at `/Users/admin/cargo-target/scribular-tauri/release/bundle/macos/Scribular.app`; pass the path printed by your own build with `--app` if it differs. `launch` reports the app path and executable modification time so stale builds are visible.

Other scenarios:

```sh
swift run --package-path tools/uitest uitest tools/uitest/scenarios/as13-dirty-close-cancel.uitest --app /path/to/Scribular.app
swift run --package-path tools/uitest uitest tools/uitest/scenarios/keychain-after-rebuild.uitest --app /path/to/Scribular.app
```

### Model cache

`launch` points the app's `models` folder (`~/Library/Application Support/com.app.scribular.tauri/models`) at a symlink to `~/Library/Caches/scribular-uitest/models`. The first run downloads about 1.85 GB of models into it; later runs reuse it and skip the download. If app data already holds a real `models` folder from a manual run, `launch` moves its files into the cache instead of into the run folder. Files already in the cache are never overwritten or deleted; the app verifies them itself.

Clear it with `rm -rf ~/Library/Caches/scribular-uitest` only to free disk space. Do not clear it to "fix" a failing test: a missing model means a 1.85 GB re-download, not a different result.

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

Each action prints one line (`labels` prints a count line plus one line per label). A failed step stops the run and prints its screenshot path and up to 40 visible labels. Screenshots are cropped to the app window, or the full screen when a keychain prompt is open or no window exists. The final line includes the result and recording path:

```text
✓ 1 launch /path/to/Scribular.app (built 2026-11-05T20:00:00Z)
✗ 6 expect "Bullet recap": not found after 10s
  screenshot: runs/<timestamp>/fail-6.png
  visible: "Settings", "Note styles", ...
FAIL 5/6 steps — recording: runs/<timestamp>/recording.mp4
```

Exit codes: `0` for pass, `1` for a failed step, and `2` for a parse or preflight error. `as12-regenerate-picker.uitest` is a placeholder until spec 09 lands. Scenario labels were taken from the app source; the manual Accessibility-tree and screen-recording checks still need to be run on a desktop session.
