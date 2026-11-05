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
| `close-window` | Click the front window's close control. |
| `handle-keychain` | If SecurityAgent has a prompt, type the environment password and choose Always Allow. |
| `quit` | Quit the app. |

## Output

Each action prints one line. A failed step stops the run and prints its screenshot path and up to 40 visible labels. The final line includes the result and recording path:

```text
✓ 1 launch /path/to/Scribular.app (built 2026-11-05T20:00:00Z)
✗ 6 expect "Bullet recap": not found after 10s
  screenshot: runs/<timestamp>/fail-6.png
  visible: "Settings", "Note styles", ...
FAIL 5/6 steps — recording: runs/<timestamp>/recording.mp4
```

Exit codes: `0` for pass, `1` for a failed step, and `2` for a parse or preflight error. `as12-regenerate-picker.uitest` is a placeholder until spec 09 lands. Scenario labels were taken from the app source; the manual Accessibility-tree and screen-recording checks still need to be run on a desktop session.
