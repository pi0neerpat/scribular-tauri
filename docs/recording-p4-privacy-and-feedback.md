# Recording PR4: privacy fault tests and AS17 feedback fixes

Spec: `plans/2026-11-26-recording-p4-spec.md` (parent `plans/2026-11-16-recording-and-session-spec.md`; acceptance AS17, AS18; R5, R15). Evidence: `plans/evidence/recording-p4-as17.md`. Shipped in PR #68 (merge `aebc9a9`; commits `6772f45` AS18 tests and checklist, `4bec110` Cubic fixes to those tests, `9404fe2` AS17 feedback fixes).

## What it does

- **AS18 fault-injection tests** prove that clipboard, attach, pause and store failures leak no note text, transcript, session title, app name or database path, in surfaced errors or the diagnostics log. Committed chunks and earlier notes survive each fault.
- **Mic Start is never blocked** by a Coach generation or a slow summary model load in flight; Start cancels them.
- **Pending audio while finalizing** (`9404fe2`): a `recording` or `finalizing` session without `hasAudio` no longer says "No audio was kept". The Audio tab stays enabled and reads "Audio will be ready when the transcript is saved."
- **dBFS meter levels** (`9404fe2`): `session://levels` carries a level mapped from -60..0 dBFS to 0..1, so normal speech sits mid-scale.
- **Debounced app-audio permission notice** (`9404fe2`): one permission failure on attach stays "pending"; the notice appears only after a second consecutive one.
- No product behavior changed in `6772f45` beyond making the clipboard command testable (`copy_text` calls `copy_with`).

## How it works

- **Clipboard** (`src-tauri/src/commands.rs:681` `copy_text`, `:686` `copy_with`): pipes text to a program; errors name the step only (for example "start clipboard: ..."), never the text. Test `as18_clipboard_faults_report_no_note_text` (`commands.rs:1843`) uses a missing program and `/usr/bin/false`.
- **Attach, pause, store** (`src-tauri/tests/session_job.rs:1245`, `as18_attach_pause_and_store_faults_keep_notes_and_log_only_metadata`): attach fault with an app that is not playing (log shows `app_source_unavailable`); pause fault via a SQLite `BEFORE INSERT` trigger on `audio_chunks` (Pause returns `RecordError::Interrupted`, session `error` is `interrupted`, the one committed chunk stays); store faults via triggers on `sessions.summary` (note edit) and `sessions.copied_at` (copy mark, asserted not to be a `StaleCandidate` rejection). It then asserts the earlier note and `copied_at == None` are unchanged, the log contains `database_error`, and none of the private strings (app, title, note text, transcript, DB path) appear in the surfaced errors or the log. The log directory is process-wide, so the test is serialised with `capture_faults_log_metadata_and_keep_committed_audio` through `LOG_LOCK`.
- **Start not blocked** (`src-tauri/src/jobs/queue_coach_tests.rs:583` `as18_in_flight_session_coach_cannot_block_mic_start`, `:615` `as18_summary_model_load_in_flight_cannot_block_mic_start`): the model call is held on a channel (no sleep), Start runs on another thread, the recorder is kept alive until the mic is checked, then the worker is joined (`4bec110`).
- **Pending audio** (`src/lib/components/session-audio.ts:60` `audioPending`, `:65` `audioUnavailable`): `audioPending` is true when `!hasAudio` and status is `recording` or `finalizing`, because keep-or-delete is decided when the transcript is saved. `audioUnavailable` still returns the seven-day-deleted text first, then returns `null` while pending, otherwise the "No audio was kept" text. `SessionAudio.svelte:113` renders the pending line. `SummaryPanel.svelte:65` uses `audioUnavailable` to enable or disable the tab.
- **Meter level** (`src-tauri/src/jobs/session/capture.rs:547` `METER_FLOOR_DB = -60`, `:551` `meter_level`): `20*log10(rms)`, scaled `(db + 60) / 60`, clamped to 0..1; non-finite or non-positive RMS gives 0. `emit_levels` sends `meter_level(level)` (`:520`). Examples: RMS 0.0316 (-30 dBFS) is 0.5, RMS 1.0 is 1.0. `hearing` still compares raw RMS to `SILENCE_RMS` (`:565`), so the warning logic is unchanged.
- **Permission debounce** (`capture.rs:30` `PERMISSION_CONFIRMATIONS = 2`, `:475` `log_attach_failure`, `:528` `app_audio_status`): `permission_failures` counts consecutive `PermissionRequired` attach failures, resets to 0 on any other failure or a successful attach (`:410`). `app_audio_status` returns `Capturing` if a span exists, `Pending` for fewer than 2 permission failures, `PermissionRequired` from the second, `Unavailable` for other errors. `recording_state` reports it (`:322`). AudioCapCLI also exits silently on transient failures, which reads as a denial; retries run every `RETRY_INTERVAL` (1 s), so the notice no longer flashes before a late attach succeeds.

## Config and data

- No new tables, settings keys or env vars. The fault triggers exist only inside the tests' temporary databases.
- `SessionLevel.level` changes meaning (dBFS-mapped 0..1 instead of raw RMS); the type is unchanged. `src/lib/types/Session.ts` was regenerated in `6772f45` (whitespace only, matches main).
- `PERMISSION_CONFIRMATIONS` and `METER_FLOOR_DB` are constants in `capture.rs`.

## How to test

- AS18 and capture unit tests: `cargo test --locked --manifest-path src-tauri/Cargo.toml --features fake-engines`.
- Single groups: `... --features fake-engines --test session_job as18`; `... --features fake-engines --lib as18`; `... --features fake-engines --lib one_permission_failure_stays_pending_until_it_repeats`; `... --features fake-engines --lib meter_level_maps_rms_to_a_bounded_dbfs_scale`.
- Frontend: `npx vitest run src/lib/components/session-audio.test.ts src/lib/components/summary-panel.test.ts` (or `npm test`). Covers `audioPending` for `recording`/`finalizing` and the "Audio will be ready..." panel.
- AS17 by hand: follow the checklist table in `plans/evidence/recording-p4-as17.md` on a real Mac.

## Known limits

- AS17 is manual-only (real macOS AudioCapCLI and System Audio permission). The build session ran in a Tart VM, so the checklist was committed as UNVERIFIED (not run). Patrick then walked it on a real Mac; his feedback produced `9404fe2`. No hardware result is recorded in the evidence file.
- Echo cancellation is out of scope (when the call app plays through speakers, the mic can pick it up).
- The permission debounce delays a real denial notice by one retry (about 1 s).
- Tab-specific capture and new UI are out of scope per the spec.
