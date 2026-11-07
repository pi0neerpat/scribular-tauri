# Diagnostics and consent-gated telemetry

## What it does
- Writes a local, content-free log file (`scribular.log`) for support.
- Settings -> "Privacy & diagnostics" offers: a "Send usage and error reports" checkbox, "Copy diagnostics", and "View logs".
- With consent only, sends coarse weekly usage numbers and generic error reports to the Scribular backend.
- Nothing clinical goes in the log or any report: error text is reduced to fixed categories (`diag::log::kind`) and fixed messages.

## How it works
- **Log** (`src-tauri/src/diag/log.rs`): format `<UTC timestamp> <LEVEL> [module] message`. A PANIC line also writes a `panic.pending` marker; on next launch (`lib.rs`) that triggers an `unhandled` error report (only if telemetry is on) and the marker is removed. Panics inside recovery regions are not logged as PANIC (`panic.rs`).
- **Frontend errors** (`commands.rs`, `report_frontend_error`): limited to 20 per 60 s (`report.rs`), one "suppressed frontend errors" WARN on overflow; only frames passing `safe_frontend_frame` (max 8) are logged.
- **Copy diagnostics** (`diag/info.rs`): 7 lines: app version (+ build SHA if set), macOS version and arch, Tauri version, DB schema version, model states, telemetry (`on`/`off`/`undecided`), log dir. The home directory is shown as `~`; a path outside home becomes `[redacted]`.
- **Usage** (`diag/telemetry.rs`): `POST {root}/api/usage` with a random device id (UUID-style, created on first need), sessions this week (capped 200), average session minutes (capped 480), launches this week (capped 500), app version, platform, week start. Sent at launch, after the first session of a week, and on quit (2 s timeout; otherwise 10 s).
- **Errors**: `POST {root}/api/errors` with context, a fixed message, version/platform/arch/code, timestamp, device id. Codes allowed: `summarize`, `recording`, `dialog`, `unhandled`, `test`. Rate limits: 30 per 60 s overall, one per code per 10 min. The last report is held as `pending_report` and retried at the next launch until a 2xx response.
- Consent is re-checked before every counter update and network send. Turning telemetry off deletes the device id, week marker and pending report, and zeroes the counters.

## Config and data
- **Log location**: `<app data dir>/logs/scribular.log` (the directory is `0700`, files `0600`). Example in tests: `~/Library/Application Support/com.app.scribular.tauri/logs`.
- **Rotation**: when the file exceeds 1 MiB it rotates to `scribular.log.1` -> `.2` -> `.3`; `.3` is dropped. Rotation is also checked at startup.
- **Consent default**: `telemetry_enabled` unset = "undecided", treated as off. Only the value `1` allows sending. Stored in the app's settings table (`telemetry_enabled`, `install_id`, `usage_*`, `pending_report`).
- **`SCRIBULAR_API_ROOT`**: honoured only in debug builds (`cfg(debug_assertions)`); if unset or empty, nothing is sent. Release builds always use `https://www.scribular.com`.
- Debug builds also echo each log line to stderr.

## How to test
- Rust unit tests in `src-tauri/src/diag/` cover rotation (1 MiB, three backups), error-kind redaction, the 7-line diagnostics output with `~` home, the frontend rate limit and panic handling: `cargo test --manifest-path src-tauri/Cargo.toml diag`.
- Manual: Settings -> Privacy -> Copy diagnostics and paste; View logs opens `scribular.log`; toggle the checkbox and confirm Telemetry flips in the diagnostics text.
- Dev-only "Developer" section ("Send test error", "Trigger test panic"): run a dev build with `SCRIBULAR_API_ROOT` pointing at a local listener (e.g. `nc -l 8080`) and check the request body.
- Spec acceptance scenarios: `plans/parity/06-diagnostics-telemetry.md` (AS10-AS13).

## Known limits
- AS10 (FirstRun consent) was not verified. The FirstRun choice is owned by parity 07; until then undecided = off and the only switch is Settings -> Privacy.
- AS12 was not manually verified. The Developer buttons are gated by `import.meta.env.DEV`, so they are absent in debug bundles built with the production frontend.
- AS13 (spoken canary name absent from the log) was not manually verified.
- `platform` in the usage payload is hard-coded `darwin-arm64`.
- Release builds cannot redirect telemetry; testing against a local server needs a debug build.
