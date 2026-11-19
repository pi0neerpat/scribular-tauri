# Recording sources and capture (recording spec, PR1)

Spec: `plans/2026-11-16-recording-and-session-spec.md` (R1, R4 to R6, R17; acceptance AS1, AS4, AS5, AS6, AS18). Shipped in PR #32 (merge `bdd0f5c`, "Add mic-first recording and pause timeline"). This is the Rust source contract only; the record-screen UI is not part of it.

## What it does

- **Source choice** is stored as `record_location` (`inRoom` default, `onCall`) plus `record_target_app`. In the room maps to `InputMode::Mic`; On a call maps to `InputMode::MicAndSystem`.
- **Mic-first start**: On a call opens the microphone immediately, even if the chosen app is silent or closed. The app source attaches when it can.
- **Late, dropped and returning app audio** join the same session. No second session is created.
- **Pause and resume** a running recording. Paused time is excluded from the saved audio, segments and duration.
- **Level events** (`session://levels`) per source, with a call-silence warning after about 20 active seconds.
- **Recording state events** (`session://recording-state`) and a `recording_state` query, so the UI can reload the true state.
- **Capture faults** keep committed chunks. A lost microphone or failed write interrupts the session, which can then be recovered.
- `system` (this-app only) can no longer be started. Old `system` sessions stay readable.

## How it works

- **Commands** (`src-tauri/src/commands.rs`): `get_recording_source`, `set_recording_source`, `recording_state`, `pause_recording`, `resume_recording` (registered in `src-tauri/src/lib.rs`). Pause and resume return a typed `RecordingControlError`: `notRecording` or `failed { message }`.
- **Start** (`Recorder::start`, `src-tauri/src/jobs/session.rs:321`): `System` returns `UnsupportedMode`; `MicAndSystem` needs a valid target app (`NoTargetApp` otherwise). A refused source leaves no session (`a_refused_source_leaves_no_session`).
- **Capture timeline** (`src-tauri/src/jobs/session/capture.rs`): the microphone is the sample clock. An app span that attaches late is placed on the mic timeline from its first block (`poll_attach`), so app chunks land at the right active offset. Chunks are written by `src-tauri/src/audio/chunk_writer.rs`; a later span extends the final partial chunk, keeping fixed-size chunk addressing.
- **Attach and retry**: the attach runs on a worker so mic start never waits for it. While recording, a failed or lost app source is retried every `RETRY_INTERVAL` (1 s). Stop cancels the attach. A failed live transcription window is replayed before reattached app audio (test `a_failed_live_window_is_replayed_before_reattached_app_audio`).
- **Pause and resume** (`Recorder::pause` / `resume`, `session.rs:372` / `:389`): pausing when already paused, or resuming when running, is a no-op returning Ok. With no recording both return `NotRecording`. Pause stops writes and live transcription for both sources and suspends app attach. Stop is accepted while paused. If pause or mic resume fails, the session is interrupted with its audio kept. If only the app fails to resume, the mic keeps running.
- **Levels and warning** (`capture.rs`, `emit_levels`, `hearing`): the watch loop ticks every `WATCH_INTERVAL` (100 ms). `SessionLevel` carries `sessionId`, `source`, `level`, `hearing` (`silent`, `hearing`, `warning`) and `activeElapsedMs`. Only the call source (`AudioSource::System`) can reach `warning`: it needs `CALL_WARNING_MS` (20,000) of active time without sound, and clears when sound returns. Elapsed time is monotonic and excludes paused time. No level history or PCM is kept.
- **Faults**: mic loss stops with "Recording stopped: the microphone was lost."; a write failure with "Recording stopped: couldn't save audio." (`capture.rs:25-26`). The session becomes `Failed("interrupted")` with its chunks kept (`Inner::interrupt`). Stopping with no audio at all fails the session. Diagnostics log event kinds and counts only (for example `app_source_unavailable`), no audio, transcript or app names.

## Config and data

- **Recording settings** (`RecordingSettings`, `src-tauri/src/store/settings.rs:50`): `inputMode`, `targetApp`, `promptId`, `keepAudio`, `multiSpeaker` (Identify speakers). Saving `inputMode: system` is rejected (`InvalidRecordingSource`); an on-call source needs a non-empty target app without NUL bytes (`valid_target_app`).
- **Recording source** (`RecordingSource { location, targetApp }`): stored in the `settings` table under `record_location` and `record_target_app`. Clearing the app deletes its key.
- **Legacy settings**: if `record_location` is absent, the old `input_mode` and `target_app` keys are read. `micAndSystem` with an app becomes On a call; anything else (including `system`) reads as In the room. On a call with no valid app also reads as In the room.
- **Legacy sessions**: `InputMode::System` rows stay in `sessions` and decode normally.
- **Audio**: 16 kHz mono chunks in the `audio_chunks` table, per source (`mic`, `system`), as described in `docs/session-management-recovery.md`.
- Generated TS types: `src/lib/types/` (`RecordingSource`, `RecordLocation`, `RecordingState`, `RecordingStatus`, `SessionLevel`, `HearingState`, `RecordingControlError`); wrappers in `src/lib/api.ts`.

## How to test

- Session and capture (fake engines, fake system source): `cargo test --locked --manifest-path src-tauri/Cargo.toml --features fake-engines --test session_job`. Covers late/returning app audio, mic-first start, pause/resume/quit while paused, mic loss recovery, fault logging.
- Store and settings: `cargo test --locked --manifest-path src-tauri/Cargo.toml --features fake-engines --test store` (`record_source_defaults_round_trips_and_keeps_legacy_sessions_readable`, `recording_settings_round_trip_and_clear`).
- Chunk writer: `--test chunk_writer`. Unit test for the warning timer: `call_silence_warns_after_twenty_active_seconds_and_hearing_resets_it` in `capture.rs`.
- Frontend: `npm test` (`onboarding-tour.svelte.test.ts`, `settings-model-progress.svelte.test.ts` were touched).
- Signed-app uitest: build and run steps in `tools/uitest/README.md` and the repo `CLAUDE.md`.

## Known limits

- The record-screen source picker and meters UI arrive in PR2; Stop handoff and the session page in PR3; the real-Mac capture and playback walkthrough (AS17) in PR4.
- AS5 (meters and amber warning with real app audio) is manual. The native meters and warning are unverified on real hardware; tests use fake sources.
- AS4, AS6 and AS18 are covered by fake-source tests, not real AudioCapCLI capture.
