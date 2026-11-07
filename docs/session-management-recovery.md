# Session management and recovery (spec 09)

Spec: `plans/parity/09-session-management-recovery.md`. Recovery and waiting-session cancel are Tauri-originated; Electron has neither.

## What it does

- **History view** (`src/lib/components/SessionHistory.svelte`): lists all sessions (no 100-session cap, unlike Electron) with a snippet, status and per-row actions. Rows whose audio was swept show "Audio no longer available" and only offer Delete.
- **Session detail**: `SummaryPanel.svelte` (mounted with `management`) shows the summary, transcript, provenance (style and model that produced the summary) and the mounted audio/speaker components.
- **Copy**: copies the summary, falling back to the transcript; disabled when both are empty; shows "Copied" briefly. Backend fallback: `copy_text` (`src-tauri/src/commands.rs`, pipes to `pbcopy`).
- **Regenerate**: style picker (`StylePicker.svelte`), streaming preview in a separate area (the stored summary is untouched until success), and Stop. Failure or Stop keeps the old summary.
- **Undo**: one snapshot per session. A successful regenerate stores the old summary, style and model; Undo restores them.
- **Delete / Delete all**: single delete and Delete all, each with a confirm step (a second click within 3 s for single delete). Delete all also removes samples and runs `VACUUM`.
- **Recover interrupted recordings**: a banner ("N interrupted recording(s)") and a Recover button per row. Recovery re-transcribes the kept audio, then summarizes.
- **Cancel awaitingModel**: a session waiting for the summary model download can be cancelled and returns to a stopped state.
- A session with too little speech (under `MIN_WORDS`) stays `Failed(InsufficientAudio)` with a Delete action; it is not silently deleted as in Electron.

## How it works

- **Commands** (`src-tauri/src/commands.rs`): `list_sessions` (:325), `delete_session` (:344), `clear_all_sessions` (:356), `copy_text` (:374), `undo_summary` (:369), `regenerate_summary` (:654), `cancel_job` (:681), `recover_session` (:686).
- **Store** (`src-tauri/src/store/sessions.rs`): `SessionListItem.snippet`; `finish_summary` copies the old summary/prompt/model into `previous_*` in the same UPDATE, only on success and only if the old summary is non-empty; `undo_summary` (:598) refuses while the session is recording, finalizing, awaitingModel or summarizing; `cancel_running_summary` (:388) and `cancel_queued_summary` (:405) (queued cancel restores the pre-regeneration status); recovery helpers `claim_recovery` (:672), `save_recovered_transcript` (:636), `set_recovery_status` (:659), `fail_recovery` (:683); `recover_interrupted` (:977) runs at launch.
- **Migration v7** (`src-tauri/src/store/migrations.rs:57`, `v7_summary_undo`): adds `previous_summary`, `previous_prompt_id`, `previous_model_id` to `sessions`. This is the single undo snapshot.
- **Jobs** (`src-tauri/src/jobs/`): `queue.rs` coordinates delete/clear-all with cancel; `session.rs` `recover` (:474) validates, claims, then spawns a `recover-session` thread. The thread clears old segments, re-transcribes all audio per source (and runs diarization when multi-speaker applies), saves the transcript keeping the audio chunks, then calls the normal regenerate. Normal retention cleanup runs only after the summary succeeds. Cancel at any stage returns the session to `Failed("interrupted")` with audio kept.
- **Frontend**: `SessionHistory.svelte` calls `listSessions`, `getSession`, `deleteSession`, `clearAllSessions`, `recoverSession`, `cancelJob` from `src/lib/api` and listens to `onSessionProgress`. `SummaryPanel.svelte` handles regenerate, stream preview, stop and undo.

## Config and data

- Sessions live in the encrypted SQLite store (`src-tauri/src/store/`). Stored per session: transcript, summary, `prompt_id`, `model_id`, `previous_*` snapshot, status/error, and (spec 02) `plain_transcript`, `raw_transcript`, `speaker_map`, `diarization_state`.
- **Audio**: recorded in 5 s chunks (16 kHz mono) into the `audio_chunks` table, so a crash loses at most about 5 s. Normal retention deletes chunks after a successful transcript/summary, unless the user keeps audio; the hourly sweep in `jobs/retention.rs` expires kept audio.
- **Recovery state** lives in the DB, not a side file: a session with status `Failed`, error `"interrupted"` and `audio_chunk_count > 0` is recoverable. At launch `recover_interrupted` marks live recordings interrupted and puts finalizing/summarizing sessions that are in `summary_queue` back to `AwaitingModel` in FIFO order (they resume).
- Recovery is manual (banner plus button), not automatic at launch, because it costs CPU.

## How to test

- Rust (fake engines): `cargo test --manifest-path src-tauri/Cargo.toml --release --features fake-engines` (recovery, cancel, undo, delete-all coverage per AS11/AS12).
- Frontend: `npm test` (vitest, includes `src/lib/components/session-history.svelte.test.ts`, `summary-panel.test.ts`); `npm run check` (svelte-check).
- Native UI: `tools/uitest/scenarios/as12-regenerate-picker.uitest` and `as13-dirty-close-cancel.uitest`; build and run steps are in `tools/uitest/README.md` and the repo `CLAUDE.md`. Start new recording tests from `system-audio-smoke.uitest`.

## Known limits

- Spec acceptance scenarios AS14 to AS18 (kill -9 recovery, waiting-session cancel/relaunch, regenerate Stop/Undo, delete confirms, expired-trial access) are manual and were never verified in the native app.
- Recovery is refused once the app is shutting down (`jobs/session.rs:476`, "app is shutting down"); an in-flight recovery is cancelled by shutdown and retried on next launch.
- Recovery needs kept audio: if retention already removed it, only Delete is offered.
- Undo holds a single snapshot per session; regenerating twice loses the older summary.
- Electron has no equivalent for recovery or waiting-session cancel, so there is no parity reference for them.
