# Session management and recovery — Tauri parity spec
Status: draft for cwf; Date: 2026-11-03


> **Dependency policy (Patrick, 2026-11-05; supersedes every dependency-review, Patrick-approval, publish-date cutoff, two-week rule, exact-pin, OSV/RustSec, owner-check or `build.rs`-review requirement in this spec and in `plans/DEPENDENCY_REVIEW_*.md`).** Before adding a crate or npm package, check only that (1) it is commonly used (substantial downloads or dependents) and (2) it has had a release or commit within the last 12 months. Record the result in one or two lines in the PR description. No `DEPENDENCY_REVIEW_*.md` doc and no approval step. Prefer dependencies already in the tree. This covers packages only: a pinned model file still needs its URL, size and sha256 taken from the upstream file, never invented.

## Intent
Users keep their meeting notes in the session list. They need to open a past session, copy the summary, regenerate it with another style, undo a bad regeneration, delete one session or all of them, and not lose a recording when the app crashes or the summary model is still downloading.

Tauri today has no session list or session actions at all, only the "Record" screen showing the current session. This spec adds a History view and the actions on it. It also adds recovery of interrupted recordings and management of waiting (awaitingModel) sessions. The last two are Tauri-originated requirements: Electron has no equivalent (see below).

## Electron behaviour today (file:line)
Paths are under `notes-assistant-electron/src/`.

**List / sidebar**
- Count label `#sessions-count` (`index.html:117`); "Shred all notes" button `#clear-all-sessions-btn` (`index.html:119`); `updateSessionCount` (`renderer/features/sidebar.js:262-274`).
- Rows: date and time headers (`sidebar.js:359-363`). Snippet = summary, else partial streaming summary, else transcript, else "No summary available", cut at 100 chars plus "..." (`sidebar.js:372-419`). Step-dot progress for in-flight sessions (`sidebar.js:285`).
- No search, no grouping beyond date headers, no rename. `loadSessions` 516-542.
- Two-step inline delete: a "Confirm" overlay that resets after 3000 ms (`sidebar.js:468-501`).
- Clear all: native `confirm("Are you sure? This will delete all sessions permanently.\n\n This action cannot be undone.")` (`sidebar.js:108-116`).

**Store** (`store/sessions.js`)
- `saveSession` is an upsert by id (208-279).
- **100-session cap**: oldest sessions are silently spliced off at 268-271, with no audio cleanup. This is data loss. Flag as buggy.
- `deleteSession` also deletes `audio/<id>.wav.enc` (284-308). `clearAllSessions` 313-338. `getSessionCount` 343-352.
- IPC: `main/registerIpcHandlers.js:119-185`; channels `common/ipcChannels.js:11-18,73-78`; preload `preload/preload.js:24-30,106-110,149-154`.

**Session view** (`renderer/views/session.js`)
- `#session-transcript` and `#session-summary` are `readonly` textareas (`index.html:405-407, 442-444`). There is no editing and no markdown rendering.
- Copy (`session.js:1296-1313`): copies the summary, falling back to the transcript. There is no "copied" feedback.
- Rewrite (`session.js:1318-1397`). Buttons `#session-rewrite-button`, `#session-undo-button` (hidden by default), `#session-copy-button` (`index.html:425-427`).
- **Undo pre-save** (`session.js:1338-1354`): `previousSummary`/`previousPromptId`/`previousModel` are saved BEFORE generation, only if the current summary is non-empty. A Stop or failure therefore leaves a redundant `previousSummary` (quirk).
- Undo handler (`session.js:1403-1446`): restores summary, `promptId = previousPromptId || promptId`, `model = previousModel || null`. It then clears all three previous* fields and hides the button. Single level, no redo. Button logic 582-587.
- Stop button (`session.js:1449-1462`) is visible only in the summarize_chunk and combine phases, not during transcription (`syncStopRequestVisibility`, 114-123). Button-disable logic 154-211. `restoreSavedSummary` 1228-1239.
- Provenance: style name or "Deleted style" (`session.js:509-534`); model name via the catalog (`session.js:498-504`).
- `loadSession` 726-887.
- Too little transcript: the session is deleted (`session.js:1503-1539`; `deleteIncompleteSession` 133-152; `recording.js:400-410`).

**Summarization transport**
- `service/summarize.js`: `cancelSummarization` 30-40, `regenerateSummary` 117-185, `sendTranscript` 236-523 (cancel 411-434, errors 436-517). One global `activeRun` with an AbortController.
- Processing state is a renderer-only, non-persisted Map (`renderer/features/sessionProcessing.js`). Nav is locked only during a model download (`renderer/ui/viewNavigation.js:16-91`).

**Chat is dead.** `renderer/features/chat.js` is unused and `setupChatListeners` is never called. The `sendChat` API (`api.js:191-239`) has no renderer caller. The `CHAT_REPLY`/`CHAT_STOP` channels are live only as the summary-stream and cancel transport (`session.js:1661-1830`). Chat is therefore **not a feature to port**. Only the stream and cancel transport matters, and Tauri already has it as `session://summary-delta` and `cancel_job`.

**Crash recovery: none.** `index.js:344-437` (`cleanupExpiredAudio`, `cleanupOrphanedTempAudio`) only deletes old or orphaned audio. Nothing turns an interrupted recording into a transcript. Electron also has no "waiting session" concept, because the model download blocks the flow. So crash recovery and reopening or cancelling a waiting session are Tauri-originated requirements, not strict parity.

## Tauri state today
Paths are under `scribular-tauri/`.

**Backend (mostly present)**
- Commands (`src-tauri/src/commands.rs`): `list_sessions` 94-97, `get_session` 99-102, `update_session` 104-111, `delete_session` 113-116 (raw store delete), `regenerate_summary(session_id, prompt_id)` 149-157, `cancel_job(session_id)` 161-164, plus the recording commands. **There is no clear-all, undo, copy or recover command.** The `EVENTS` list is at 28-34 and `export_ipc_types` at 64-82.
- `store/sessions.rs`:
  - `SessionStatus` 15-23. `SessionListItem` {id, title, createdAt, status, durationMs} 111-118. There is no snippet, no count and no cap. `list_sessions` 328-345 orders by created_at DESC.
  - `Session` 122-137. `SessionPatch` 142-152.
  - `finish_summary` 407-435: one UPDATE sets summary, prompt_id, model_id, timings, status Done and error NULL. It is the natural place to capture `previous_*`.
  - `save_transcript` 454-468 also deletes `audio_chunks` in the same transaction.
  - `delete_session` 472-474: cascades to segments, audio_chunks and summary_queue.
  - `audio_samples(id, source)` 536-555, `audio_chunk_count` 557-565, `segments()` 499.
  - `recover_interrupted` 569-595, run from `Store::open` (`store/mod.rs:82`): queued Finalizing/Summarizing → AwaitingModel; any other Recording/Finalizing → `Failed` with error `"interrupted"` (const `INTERRUPTED`, line 164). **Audio chunks are kept, but nothing ever transcribes them.**
- `jobs/queue.rs`:
  - `regenerate` 129-186 errors `Busy` if the status cannot transition to Summarizing. It sets `AwaitingModel` if the loader isn't ready, and persists to `summary_queue` with a `restore_status`.
  - `cancel` 201-226 drops a pending job and restores `restore_status` (done, cancelled or failed), or flags the running job.
  - A shutdown while running keeps the job as `Summarizing`, so it resumes. `new` restores `queued_summaries()`.
- `jobs/state.rs:9-18`: `can_transition` matrix.
- `jobs/summarize.rs`:
  - `Reporter::stop` 77-91 leaves the stored summary untouched, so **a Stop or failure never destroys the old summary**.
  - `finish_summary` overwrites only on success.
  - `summary-delta` events are emitted only in `final_pass`.
  - Under `MIN_WORDS=10` (`text/chunk.rs:12`) the session ends `Failed(InsufficientAudio)` and **is kept**, unlike Electron.
- `jobs/session.rs`:
  - `Recorder::stop` 295-357, `cancel` 361-367, `Drop` 370-392 (quitting leaves Recording/Finalizing, which the next launch marks interrupted).
  - `finalize` 591-632: drain, `transcript()` (691-720), `save_transcript`, then `jobs.regenerate(prompt_for(id))`.
  - The `Stream`/`Window`/`window()` helpers do 15 s windowed transcription and are reusable for recovery.
- `audio/chunk_writer.rs`: `CHUNK_SAMPLES` = 5 s of 16 kHz mono per row, so a crash loses at most about 5 s.
- `store/migrations.rs:11`: `MIGRATIONS = [v1_initial, v2_summary_queue, v3_queue_restore_status]`, append-only.

**Frontend (mostly missing)**
- `src/App.svelte`: tabs only "Record" and "Settings". **No history UI.** `listSessions` and `deleteSession` wrappers exist in `src/lib/api.ts` (~lines 31, 44) but nothing uses them.
- `RecordView.svelte`: shows only the current session. A Cancel button appears only while `working`. `isWorking` (`record-status.ts:5-7`) **excludes `awaitingModel`**, so a waiting session cannot be cancelled from the UI. It never subscribes to `onSummaryDelta`. It keeps `scribular.inFlightSession` in sessionStorage across window reloads.
- `SummaryPanel.svelte` (29 lines): display only (title, status line, summary and transcript in `<pre>`). **No copy, regenerate, undo, delete or cancel.**
- Plans: the prototype plan R6 (line 61) says "browse/open/edit/regenerate sessions"; line 130 defers crash recovery ("Audio survives per R8, but turning it into a transcript is deferred"); KTD7 (179) persists the queue; `TAURI_MVP.md:106-108` says "Stop during Summarizing → put back the previous summary".

## Goals / Non-goals / Anti-goals
**Goals**
- History list with count, snippets and live status; session view with copy, regenerate, stop, undo, delete; clear all.
- Recover interrupted recordings from their kept audio.
- Reopen, view and cancel waiting sessions.

**Non-goals**
- Chat: dead in Electron, not ported.
- Session rename, search, grouping, markdown rendering and transcript/summary editing UI. None exist in Electron (see Out of scope). The `update_session` patch API stays.
- Multi-level undo or redo.
- Audio playback and retention (spec 01).

**Anti-goals**
- Do not port the silent 100-session cap (`sessions.js:268-271`).
- Do not delete sessions with too little transcript (Electron does, `session.js:1503-1539`).
- Never blank or overwrite a stored summary until a new one succeeds.
- Never gate any action here on licence or trial state (spec 04).

## Requirements
**History list**
- R1. Add a "History" tab. Rows are newest first and show date and time, a status badge (or step progress for in-flight sessions), and a snippet. The snippet is the summary, else the transcript, else "No summary available", cut at 100 chars plus "…". It is computed in Rust (`substr` in SQL) so the list never ships full texts. Add `snippet: String` to `SessionListItem`.
- R2. Show a session count ("N sessions"). Add `count_sessions` or derive it from `list_sessions`. Spec author's call: derive from the list, which is cheaper than a new command.
- R3. The list updates live from `session://progress` (row status, new sessions). While summarizing, show "Summarizing…" instead of partial text. If an event has an unknown session id, refresh with `list_sessions` and add the row from that result; no row metadata is added to the event payload. Spec author's call: the Tauri stream only emits in `final_pass`, so partial text would flicker.
- R4. Clicking a row opens the session view. Spec author's call: the History tab shows a list and a detail pane, reusing `SummaryPanel`.

**Session view actions**
- R5. **Copy**: copy the summary, falling back to the transcript. Disabled when both are empty. Show "Copied" for about 2 s (Electron gives no feedback; Spec author's call: cheap and expected). Use `navigator.clipboard.writeText`. If that is unavailable in the webview, fall back to a `copy_text` command that pipes to `/usr/bin/pbcopy`, in the same style as `open_privacy_settings`. No new dependency.
- R6. **Regenerate**: a style picker plus a Regenerate button. Use spec 03's `StylePicker` when it lands; until then a `<select>` of `listPrompts`. Disabled while the session is recording, finalizing, summarizing or awaitingModel, and when the transcript is empty. It calls the existing `regenerate_summary`. The session's `promptId` and `modelId` update on success only.
- R7. **Stop / Cancel**: show Stop while summarizing and Cancel while awaitingModel, both via `cancel_job`. This is better than Electron, which allows Stop only in the chunk and combine phases (`session.js:114-123`). After Stop, failure or Cancel, the previous stored summary remains visible because it is replaced only on success. Cancelling a queued regeneration restores its pre-regeneration status from `restore_status`; cancelling a running regeneration leaves status `Cancelled` (the current `cancel_running_summary` behavior). The streamed text goes in a separate preview area, never over the stored summary. Add an `onSummaryDelta` listener in the view.
- R8. **Undo** (single level, no redo):
  - A new migration (next free version at implementation time; 01, 02 and 07 land first and add migrations, so expect v7 or later) adds nullable `previous_summary`, `previous_prompt_id` and `previous_model_id` to `sessions`. Add the three fields to `Session`, so ts-rs regenerates `Session.ts`.
  - `finish_summary` copies the old `summary`, `prompt_id` and `model_id` into `previous_*` in the same UPDATE, only when the old summary is non-empty and only on success. This fixes Electron's pre-save quirk (`session.js:1338-1354`).
  - New `undo_summary(session_id)` restores the summary, and `prompt_id = previous_prompt_id ?? prompt_id`, `model_id = previous_model_id` (null if absent, as `session.js:1403-1446`). It then clears the three previous* fields. It errors `Busy` while the session is working.
  - The Undo button is visible only when `previous_summary` is non-empty.
- R9. **Provenance**: show "<style name> · <model display name>", or "Deleted style" when `promptId` no longer resolves (`session.js:509-534`; spec 03 R22 supplies model names).
- R10. **Delete one**: a two-step inline confirm (click, then "Confirm" within 3 s, else reset), as `sidebar.js:468-501`. A new `delete_session_safe` path (replacing the raw `delete_session` command's behaviour):
  - Recording or finalizing: refuse with a clear error. Spec author's call: the user should Stop first.
  - Summarizing or awaitingModel: cancel the job, then delete.
  - A running job whose row vanished is tolerated: no panic, `get_session` None → skip, `finish_summary` NotFound → no crash, and the UI ignores events for unknown ids.
- R11. **Clear all**: a "Delete all sessions" action with a confirm dialog (same wording as `sidebar.js:108-116`). New `clear_all_sessions` command. It cancels queued and running jobs. It skips sessions that are recording or finalizing and reports how many were skipped. It deletes everything else, then runs `VACUUM` (spec 01 R18). It returns the number deleted.
- R12. **No session cap.** Spec author's call: the Electron cap is silent data loss. See Decisions.

**Recovery**
- R13. **Recover interrupted**: a new `recover_session(session_id)` command, valid only when status is `Failed`, error is `"interrupted"` and `audio_chunk_count > 0`.
  - It sets `Finalizing` and spawns a thread that clears the session's old segments, re-transcribes all `audio_samples` per source with the `Stream`/`window()` helpers, and honours the finalizing cancel flag. When the multi-speaker setting applies, it runs the same diarization stage as normal finalization and persists `plain_transcript`, `raw_transcript`, `speaker_map` and `diarization_state` per spec 02.
  - It builds the transcript and calls a recovery-specific save with duration computed from the sample count; this save keeps the audio chunks. It then calls `jobs.regenerate(prompt_for(id))`. Only after the summary succeeds does it apply the normal retention cleanup; cancellation or summary failure before success keeps the chunks for another recovery attempt.
  - Below `MIN_WORDS`, it ends `Failed(InsufficientAudio)` and keeps the audio.
  - Cancelling during finalization or the queued/running recovery summary returns the session to `Failed("interrupted")` with the audio kept, including after the recovery-specific transcript save, so it remains recoverable.
- R14. A History row for an interrupted session shows a "Recover" button. At launch, a banner reads "N interrupted recordings — recover?" when any exist. Recovery is manual by default (see Decisions).
- R15. If the audio is gone (retention sweep from spec 01), the row says "Audio no longer available" and offers only Delete.
- R16. Recovery works even when the trial has lapsed (spec 04).

**Waiting sessions**
- R17. History shows awaitingModel rows as "Waiting for the summary model download…" with a Cancel button (`cancel_job`, which restores `restore_status` or sets Cancelled). Reopening one shows the same view. When the model becomes ready, the queue resumes and the row updates through progress events.
- R18. Fix `isWorking`/`RecordView` so awaitingModel also shows a Cancel button on the Record screen.

**Insufficient transcript**
- R19. A session with too little speech stays as `Failed` with the insufficient-audio message and a Delete action. Spec author's call: safer than Electron's silent delete, and the user can see why.

## Acceptance scenarios
- AS1 (automated, cargo test). Given three sessions whose summaries are long, empty-with-transcript, and empty-both, when `list_sessions` runs, then the snippets are the first 100 chars plus "…", the transcript prefix, and "No summary available", newest first.
- AS2 (automated, vitest). Given a list with a failing session, when a `session://progress` event for an unknown new id arrives, then `list_sessions` is refreshed and the returned row is added with the count incremented.
- AS3 (automated, vitest). Given a session with a summary, when Copy is clicked, then `clipboard.writeText` gets the summary and the label shows "Copied", reverting after about 2 s. Given an empty summary, the transcript is copied. Given both empty, the button is disabled.
- AS4 (automated, cargo test / fake-engines). Given a Done session with summary S1, when regenerate succeeds with S2, then `previous_summary` = S1 and the previous prompt and model ids are stored. When `undo_summary` runs, summary = S1, the prompt and model are restored, and the previous* fields are NULL. A second undo errors.
- AS5 (automated, fake-engines). Given a Done session, when regenerate is cancelled mid-run or the engine fails, then the summary is still S1 and `previous_summary` is unchanged (not set).
- AS6 (automated, cargo test). Given a Done session with an empty summary, when regenerate succeeds, then `previous_summary` stays NULL and no Undo is offered.
- AS7 (automated, fake-engines). Given a session in awaitingModel (loader not ready), when `cancel_job` runs, then the status returns to its `restore_status` and the `summary_queue` row is gone.
- AS8 (automated, fake-engines). Given a session summarizing, when `delete_session_safe` runs, then the job is cancelled, the row and its children are gone, the worker does not panic, and a later `finish_summary` for that id returns NotFound without crashing.
- AS9 (automated, cargo test). Given a session in recording or finalizing, when delete is requested, then it is refused and the row stays.
- AS10 (automated, cargo test). Given 5 sessions (one recording, one summarizing, three done), when `clear_all_sessions` runs, then 4 are deleted and 1 skipped (reported), the queue is empty and `VACUUM` ran (spec 01 R18).
- AS11 (automated, fake-engines). Given a session with 12 audio chunks and status `Failed("interrupted")`, with normal retention configured to remove audio, when `recover_session` runs, then it goes Finalizing → Summarizing → Done, with a transcript, a summary and audio_chunks deleted after summary success. With multi-speaker enabled, the recovery also saves `plain_transcript`, `raw_transcript`, `speaker_map` and `diarization_state` as in spec 02. With retention configured to keep audio, chunks remain. If summary fails before success, chunks remain. Given too little speech, it ends `Failed(InsufficientAudio)` and the chunks remain. Given a session that is not interrupted, it errors.
- AS12 (automated, fake-engines). Given a recovery in progress, when `cancel_job` runs during finalization or its queued/running summary, then the session returns to `Failed("interrupted")` with the audio kept, including after the recovery-specific transcript save, and it can be recovered again.
- AS13 (automated, cargo test). Given a database where a session was left Recording with chunks, when the Store reopens, then it is `Failed("interrupted")` with chunks kept, and the History list reports it as recoverable.
- AS14 (manual, real app). Given a recording of about 30 s, when the app is killed with `kill -9` and relaunched, then the banner shows "1 interrupted recording"; pressing Recover produces a transcript and summary; the loss is under about 5 s of audio.
- AS15 (manual, real app). Given the summary model is not downloaded, when a recording is stopped, then History shows a waiting row; Cancel returns it to a stopped state; quitting and relaunching with a waiting session leaves it waiting, and it completes after the download.
- AS16 (manual, real app). Given a Done session, when Regenerate with another style is pressed and Stop is clicked mid-stream, then the old summary is displayed untouched and Undo is not shown. When it is allowed to finish, Undo appears and restores the old summary and style label.
- AS17 (manual, real app). Given History with 3 sessions, when Delete is clicked, it needs a second click within 3 s; after 3 s it resets. "Delete all" shows the confirm text and, on accept, empties the list with the count at 0.
- AS18 (manual, real app). Given an expired trial (spec 04), all of the above still work.

## Design notes
**Rust**
- `store/migrations.rs`: new migration (next free version at implementation time) adding `previous_summary`, `previous_prompt_id`, `previous_model_id`. Specs 01 (`audio_kept`), 02 (diarization columns) and 07 (`is_sample`) add migrations before this one; 04 needs none. Keep it append-only.
- `store/sessions.rs`: `SessionListItem.snippet`; `finish_summary` captures previous*; new `undo_summary`, `delete_all_except(live_ids)` and `interrupted_with_audio()`; `Session` gains the three fields.
- `jobs/queue.rs`: `delete`/`clear_all` coordinate with `cancel`. `jobs/session.rs`: `recover(id)` with a thread, the finalizing-cancel flag and a shared transcription/diarization helper with `finalize`; recovery uses a save path that retains audio until summary success, then applies normal retention cleanup.
- `commands.rs`: add `undo_summary`, `clear_all_sessions`, `recover_session`, `list_recoverable_sessions`, `copy_text` (fallback only), and change `delete_session` to the safe path. Add any new payload types to `export_ipc_types`; add no new events (reuse `session://progress`).
- Sequence note: specs 01 and 09 both touch `save_transcript` and `recover_interrupted`. Land 01 first, or coordinate the PRs.

**Frontend**
- New `SessionList.svelte` and `SessionView.svelte` (or a History view that wraps them). `SummaryPanel.svelte` gains copy, regenerate, stop/cancel, undo and delete buttons plus a provenance line. `App.svelte` adds the "History" tab; `RecordView.svelte` and `record-status.ts` fix the awaitingModel cancel. `api.ts` wrappers: `undoSummary`, `clearAllSessions`, `recoverSession`, `listRecoverableSessions`, `copyText`.
- Use the `StylePicker` from spec 03 when it lands.

## New dependencies
None. Candidate considered and rejected: `tauri-plugin-clipboard-manager`. The in-house alternative is `navigator.clipboard` plus a `/usr/bin/pbcopy` command. If Patrick prefers the plugin, it needs only the dependency policy check at the top of this spec.

## Out of scope
- Chat (dead in Electron).
- Rename, search, date grouping, markdown rendering and transcript/summary editing UI (none exist in Electron).
- Audio playback and retention (01), diarization and speaker labels (02), the style editor (03), licence gating (04), updates (05), telemetry (06), sample sessions (07), tray and notifications (08). Their **logic** is out of scope; **mounting** their existing components in the History list and session detail is in scope (see Coordination notes).

## Depends on / interacts with
- 01-encryption-audio-retention: `VACUUM` after clear-all (R18), the 7-day retention window for interrupted audio (R16), shared `save_transcript` / `recover_interrupted`, and migration numbering.
- 03-note-styles-model-choice: `StylePicker`, `regenerate_summary` semantics and the model-name label (R22).
- 04-licensing-trial: existing sessions are never gated.
- 05-auto-update: a kill during install is recovered by this spec's flow, not by 05.
- 02, 06 and 08: 02 speaker labels appear in recovered transcripts; 06 may log recover/undo events; 08's notification on summary completion applies to recovered sessions too.

## Coordination notes (gap-check 2026-11-04)
- **09 is the integration point for the session UI.** 01-08 have all landed and mounted their pieces on the current-session result (`RecordView`/`SummaryPanel`) because no session detail view existed. Here, mount them in the History list and session detail, and re-run the UI checks those specs deferred:
  1. 01: `SessionAudio` + "Recording deletes <date>" line (01 AS20 "leaving the session", AS22 "player is gone").
  2. 02: `SpeakersCard`/`SpeakerBar` (02 AS13 "add speakers later" from detail).
  3. 03: `StylePicker` for regenerate (R6 uses it; 03 has landed, so no `<select>` fallback) + provenance/model label (03 AS12 UI).
  4. 07: `SampleBadge` in list rows, `SampleBanner` + Regenerate disabled in detail (07 AS11 badge, AS13), and the `data-tour="sessions"` anchor so tour step 3 returns.
  5. 08: fill the nav store's `session-detail` view and honour `navigationLocked`; 04: AS15 "past sessions open while lapsed".
- **Clear all** also removes samples (07 R12) and runs `VACUUM` (01 R18). **R15** "Audio no longer available" relies on 01's sweep. Electron has no recovery; R13-R16 are Tauri-originated (no parity gap).
- **Settings:** any 09 setting goes into 08's category layout.

## Decisions for Patrick
1. **Session cap.** Recommended default: no cap. Electron silently drops the oldest after 100 (`sessions.js:268-271`).
2. **Recovery trigger.** Recommended default: manual (banner plus a Recover button), because recovery transcribes minutes of audio and costs CPU at launch. Alternative: automatic at launch.
3. **Insufficient transcript.** Recommended default: keep the session as Failed (differs from Electron's silent delete).
4. **Clear-all while recording.** Recommended default: skip live sessions rather than block the whole action.

## Size and PR split
Size: M, about 6 to 8 days.
1. PR1 (about 2 days): migration, `snippet`, safe delete, `clear_all_sessions`, History list and session view with copy and delete.
2. PR2 (about 2 days): regenerate, stop/cancel, awaitingModel cancel, undo and provenance.
3. PR3 (about 3 days): `recover_session`, the launch banner and tests (AS11 to AS14).
