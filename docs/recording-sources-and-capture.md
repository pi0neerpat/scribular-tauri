# Recording sources and capture (recording spec, PR1 and PR2)

Spec: `plans/2026-11-16-recording-and-session-spec.md` (R1, R4 to R6, R17; acceptance AS1, AS4, AS5, AS6, AS18). Shipped in PR #32 (merge `bdd0f5c`, "Add mic-first recording and pause timeline"). PR1 is the Rust source contract; the record-screen UI is PR2 (see "Record screen (PR2)" below).

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

- Stop handoff and the session page are PR3; the real-Mac capture and playback walkthrough (AS17) is PR4.
- AS5 (meters and amber warning with real app audio) is manual. The native meters and warning are unverified on real hardware; tests use fake sources.
- AS4, AS6 and AS18 are covered by fake-source tests, not real AudioCapCLI capture.

## Record screen (PR2)

Shipped in PR #34 (merge `328915c`, "move recording controls onto record screen"). Spec: R7 to R9 (live transcript fold, Stop popover) and R5/R6 UI; AS8 for the transcript fold. Source choice, pause and meters moved from Settings to the Record screen.

### What it does

- **Source picker** (`src/lib/components/RecordView.svelte`): a "Recording source" radio group (In the room / On a call), shown only while not recording. On a call shows an `App:` button that opens the app picker. The picker lists common apps (Zoom, Microsoft Teams, FaceTime, Slack, Discord, Google Chrome, Safari, Firefox) plus "Other App" entries currently playing audio, with Refresh and Close. A browser app shows "Audio from all browser tabs will be recorded."
- **First-call sheet**: choosing On a call with no saved app opens a dialog (macOS asks for System Audio Recording access when capture starts) with Choose an app and Not Now. Not Now returns to In the room.
- **Start** is disabled while busy, until the source is loaded, with microphone access denied, with On a call and no app, until speech is ready, or without access. Start maps In the room to `mic` and On a call to `micAndSystem`; On a call with no app throws "Choose an app before recording a call."
- **While recording**: Pause/Resume, Stop, a Microphone `<meter>`, and with two sources an App audio `<meter>`. The `hearing: warning` level on the app source shows "No app audio heard yet. Your microphone is still recording." The clock shows active time (paused time excluded) from `session://levels`, or `activeElapsedMs` from `recording_state` while paused.
- **Stop with Identify speakers on** opens the "How many people spoke?" dialog (2 people, 3 people, 4+ beta, Keep recording). Escape closes it and keeps recording; Enter on the dialog selects 2. The dialog now renders outside the main `<section>`, which is `inert` while it is open.
- **Style picker** stays on the Record screen (moved below the Start/Stop controls).
- **Settings > Recording** no longer has the source radios or app picker. It keeps Multi-speaker and Keep audio and saves them with `set_recording_preferences`.
- **Live transcript** (`src/lib/components/LiveTranscript.svelte`): starts visible with a Hide/Show transcript button and no word count. It folds automatically only once it has shown text for 20 active seconds (`shownAtMs`, since transcription arrives in batches) and either `activeElapsedMs >= 60_000` or at least 20 words from each selected source (mic only when not `labeled`). Non-speech tags do not count. A manual Hide/Show overrides the fold for the rest of that session. It resets when `sessionId` changes.
- **System Audio denial**: `PermissionNotice.svelte` shows "Reopen System Audio Settings" for `systemAudio`, and RecordView adds Retry app audio, which reopens the picker and re-lists apps. A successful listing clears the denial.

### How it works

- **App listing and permission** (`src-tauri/src/audio/system.rs`, `AudioCapCli::list_apps`, :152): runs `AudioCapCLI --list-sources`. On a non-zero exit: known stderr markers ("Error reading process list", "Usage:", `dyld[`, and others via `source_list_failure`) map to `AudioError::Device` with a specific message first; otherwise "permission denied" in stderr, or an empty stderr, maps to `AudioError::PermissionDenied(Privacy::SystemAudio)`; anything else is the generic "Couldn't list app audio sources" Device error. The empty-stderr rule exists because AudioCapCLI exits 1 silently when macOS denies System Audio permission. Marker lines win over "permission denied" text, so `Usage: ... permission denied` is a reinstall error. Unit test: `source_list_distinguishes_empty_permission_and_safe_failures`.
- **Command**: `list_audio_apps` (`src-tauri/src/commands.rs:1252`) runs the listing on `spawn_blocking`. The frontend maps the error text to a pane with `deniedPrivacy`.
- **Preferences command**: `set_recording_preferences(keep_audio, multi_speaker)` (`commands.rs`, registered in `lib.rs`; `setRecordingPreferences` in `src/lib/api.ts`) writes only `KEEP_AUDIO` and `MULTI_SPEAKER` in one transaction (`Store::set_recording_preferences`, `src-tauri/src/store/settings.rs`), so a Settings save cannot overwrite the Record screen's source. Enabling multi-speaker is rejected until the speaker model (`SPEAKER_CAMPPLUS`) is Ready.
- **State**: on mount RecordView loads `get_recording_source`, `recording_state`, and listens to `session://recording-state` and `session://levels`. A reload mid-recording restores the true state from `recording_state`.
- **License skip** (`src-tauri/src/license/mod.rs`): `Licensing::state()` now reports a Trial state when the dev skip is on and access would otherwise be denied, so UI gates unlock too (before, only `require_access` honoured it). `SCRIBULAR_SKIP_LICENSE` (any non-empty value other than `0`) is read only in debug builds; release builds never read it (`dev_skip`, `skip_requested`).

### Config and data

- No new tables or settings keys. Source: `record_location` and `record_target_app`; preferences: keep-audio and multi-speaker keys, all in the `settings` table.
- Env var: `SCRIBULAR_SKIP_LICENSE=1` (debug builds only).
- `sessionStorage` keys in RecordView track the in-flight session across a window reload (`IN_FLIGHT_KEY`, `IN_FLIGHT_START_KEY`, `IN_FLIGHT_SPEAKERS_KEY`).

### How to test

- Frontend: `npm test` (new or changed: `live-transcript.svelte.test.ts`, `record-view-speaker-count.svelte.test.ts`, `settings-model-progress.svelte.test.ts`, `license-gate.svelte.test.ts`, `first-run.test.ts`); `npm run check`. Cases include first-call sheet and Not Now, persisting a chosen app, picker inactive during a slow Start, System Audio denial clearing after a successful listing, short transcript staying open after a long pause, Escape closing the speaker dialog.
- Rust: `cargo test --locked --manifest-path src-tauri/Cargo.toml --features fake-engines --test store` (`recording_preferences_do_not_replace_source`); `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib` for the `system.rs` and `license` unit tests (`skipping.state()` assertion).
- Native UI: build and run steps in `tools/uitest/README.md`. Existing scenarios `as11-note-style-default.uitest` and `system-audio-smoke.uitest`; there is no scenario for the record-screen picker, meters, pause or transcript fold yet.
- **Manual only: System Audio permission denial.** Deny System Audio Recording for Scribular (System Settings > Privacy & Security), choose On a call, open the app picker. Expect the "App audio permission is required or unconfirmed." notice with Reopen System Audio Settings and Retry app audio. Grant access, press Retry app audio, and the notice clears. Cannot be automated because the real AudioCapCLI is needed to exit 1 with empty stderr; the unit test only fakes it with a shell script.
- Manual only: meters and the amber warning with real app audio (AS5).

### Known limits

- `tools/uitest/scenarios/system-audio-smoke.uitest` still selects the app through Settings ("App audio" and `pick-app`), which this PR removed from Settings; it needs updating to use the Record screen picker before it can be relied on.
- The picker's common-app list is a fixed frontend list and is offered whether or not the app is installed or playing.
- Selected app is matched by name; the capture follows the process running at Start (see the uitest README on `afplay` loops).
- The auto-fold needs 20 active seconds of visible text, so a transcript never hides sooner, even at 20 words per source.
- Native behaviour of the new screen (picker, meters, pause, fold) is covered by component tests only, not by a signed-app uitest.

## Stop handoff, session tabs and playback (PR3)

Spec: R10 to R16, AS7 and AS10 to AS16.

### What it does

- **Stop handoff**: Stop opens the returned session in history at once. Its progress and transcript update there while the Record screen is free to start another recording (`RecordView.svelte` `onStopped`).
- **Style at Stop** (AS7): the style active when Stop is pressed is used for the note, even if the global style changes while the session waits for the summary model.
- **Note-first tabs** (`src/lib/components/note-tabs.ts`, `SummaryPanel.svelte`): Note, Transcript and Audio. Note is always the initial tab. Copy Transcript stays available when the note is unavailable.
- **Edit, rewrite, Undo/Redo**: the note textarea saves on blur and before a rewrite. Picking a style rewrites the note. Undo and Redo are on the note line and in Edit > Undo/Redo (`shell-undo`/`shell-redo` in `src-tauri/src/shell/menu.rs`; a focused text field keeps native undo). Lapsed access allows view and copy only.
- **Copy**: "Copied" shows only after a successful Copy Note; any edit, rewrite or Undo clears it. Copy Transcript never sets it.
- **Speakers**: Edit Speakers opens the form; pending names survive closing and reopening and apply only on Done.
- **Playback** (`SessionAudio.svelte`, AS15): Play/Pause, Back/Forward 15 seconds, a Playback position slider, Speed, and a Timed transcript list that highlights the current line and seeks on click. Lines come from `get_session_segments` (both sources merged in time order).

### How to test

- `npm test` (`summary-panel.test.ts`, `speakers-card-interactions.svelte.test.ts`, `session-history.svelte.test.ts`, `session-audio.test.ts`); `npm run check`.
- Rust: `session_job.rs::as7_a_later_global_style_change_leaves_the_queued_note_alone`, `store.rs::session_segments_merge_sources_in_time_order`.
- Native: scenarios AS10 to AS16 live in the cwf task's `scenarios/` folder. AS16 uses the new uitest `force-quit` command to leave an interrupted recording.
- AS15 playback (audible audio, speed, seeking) needs a real-app check.
