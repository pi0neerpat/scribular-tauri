# uitest doctor `--no-prompt`

## What it does
`uitest doctor --no-prompt` is a preflight for UI test runs. It skips the System Events Automation probe (the only check that can raise a macOS prompt) and reports the rest: Accessibility, Screen Recording, `ffmpeg`, stored keychain password, and the app bundle when `--app` is given. Skipped checks print `not checked`.

## How it works
- Checks live in `tools/uitest/Sources/UITestCore` behind injectable probes; the CLI in `Doctor.swift` only prints.
- Each check prints `ok` or `MISSING` with a fix command. Exit 1 if any check is MISSING, else 0.
- Inside tmux, a missing grant names the process that owns the tmux server. If that server was started outside a recognized terminal app (Terminal, iTerm2), doctor prints a restart command so the server inherits a terminal's grants.
- Under launchd or any context with no tmux grant, Accessibility and Screen Recording report MISSING and exit 1. That is correct, not a bug.

## Config and data
No config. Needs the debug build described in `tools/uitest/README.md` (Build and run). `SCRIBULAR_SKIP_LICENSE=1` works for debug builds only.

## Test it
`swift test --package-path tools/uitest` (86 tests at merge), then from a tmux session:
`swift run --package-path tools/uitest uitest doctor --no-prompt --app "$CARGO_TARGET_DIR/debug/bundle/macos/Scribular.app"`.

## Known limits
- The per-build keychain trust (cdhash) cannot be detected; doctor only prints the trust command.
- `--no-prompt` does not verify Automation, so a later `launch` may still prompt once.
