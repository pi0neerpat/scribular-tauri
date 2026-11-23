# Session Coach conversation (PR #53)

## What it does
- Every session note view with a saved note shows a **Coach** composer below the note (`SessionCoach.svelte`, mounted from `SummaryPanel.svelte`). The user types a request such as "Make it shorter" and the coach writes one full replacement note as a pending candidate. It never writes the session by itself.
- The candidate is shown with **Changes** (line diff), **Before** (saved note) and **After** (candidate) tabs, plus **Use This** and **Keep the Old One**. A follow-up request is sent with the earlier requests and candidate as context but produces one replacement against the saved note.
- **Use This** is guarded: the client sends only the session id and an opaque candidate id. The backend accepts only if the saved `summary`, `prompt_id` and `model_id` still match the candidate's base; otherwise the user gets a stale error and must ask again. Acceptance keeps the style provenance and clears `copied_at`.
- **Stop** (the send button while a job runs) cancels only the coach job, applies nothing, shows "Stopped.", and restores the earlier candidate and its actions if the replacement had not arrived yet.
- Accepted edits use the shared note **Undo / Redo** in `SummaryPanel.svelte` (no second stack). Both persist after leaving the session and after relaunch. Copy Note stores a copy marker (`copied_at`) via `markNoteCopied`; the marker is cleared by accept, Undo and a new summary, and is only set when the saved note still matches.
- Chat, input and pending candidate live only while the session stays open (`{#key session.id}`).
- **Availability:** the composer is disabled with a placeholder message and no banner when the shared `CoachAvailability` reason applies (busy, downloading, unsupported, load failed, license ended, recording). A running job is stopped when the gate closes; the note stays readable and Copy Note still works.

## How it works
- **Commands** (`src-tauri/src/coach/session.rs`, registered in `src-tauri/src/lib.rs`, wrappers in `src/lib/api.ts`): `session_coach_start` (returns a job id), `session_coach_stop`, `session_coach_accept`.
- **Events:** `session-coach://result` (`SessionCoachResult`: a `candidate` with id and note, or a `reply` message) and `session-coach://stopped`. Results carry the job and session id, so a late result cannot appear in another session.
- **State:** one in-memory `SessionCoach` holds the active job, the current candidate and the previous candidate (`restore_after_stop`). Nothing is persisted.
- **Job:** `SessionCoachJob` in `src-tauri/src/jobs/queue.rs` runs on the same queue and summary model as real summaries. A real summary closes the gate, and `cancel_for_gate` / starting a recording cancels coach work.
- **Request support and checks:** `supported` accepts length, quote, section and bullet requests; anything else returns the `UNSUPPORTED` reply ("I can change length, quotes and sections..."). Targeted section edits (`revise_section`, `remove_sentence`) change only the asked section. `grounded` rejects output that adds words not in the transcript or note. A failed or ungrounded result returns the `FAILED` reply and leaves the note and earlier candidate intact.
- **Accept:** `Store::accept_coach_candidate` (`src-tauri/src/store/sessions.rs`) is one guarded `UPDATE` inside `JobQueue::while_real_idle` that moves the note to `previous_*`, clears `redo_*` and `copied_at`, and returns `StaleCandidate` on a mismatch. Undo and redo are `undo_summary` / `redo_summary` in the same file.
- **Frontend types** are generated into `src/lib/types/` (`SessionCoachResult`, `SessionCoachOutcome`, `SessionCoachStopped`).

## Config and data
- Migration `v9_note_redo` (`src-tauri/src/store/migrations.rs`) adds `sessions.redo_summary`, `redo_prompt_id`, `redo_model_id` and `copied_at`. Undo uses the `previous_*` columns from `v7_summary_undo`.
- The coach reads the transcript and note locally. Requests, notes and candidates are not stored, logged or sent over the network.
- No new settings.

## How to test
- Rust: `cargo test --manifest-path src-tauri/Cargo.toml --lib` (add `--features fake-engines` as in `docs/note-style-coach.md` for queue tests). Unit tests are in `src-tauri/src/coach/session.rs`: targeted edits, Stop restoring the previous candidate, grounding.
- Frontend: `npm run check && npm test` (`src/lib/components/session-coach.svelte.test.ts`, `summary-panel.test.ts`, `api.test.ts`).
- Native: no uitest scenario covers the Session Coach yet (`tools/uitest/scenarios/` has only `note-style-coach.uitest` for the style coach). A real-model run is manual; see `CLAUDE.md` "E2E / uitest" and `docs/remote-llm-uitest.md`.

## Known limits
- The style offer (Update Style / Just This Note, R11 to R14 in `plans/2026-11-16-session-coach-spec.md`) ships in PR3. This PR has no offer after accepting or copying.
- Only the request words in `supported` start a revision; other requests get the unsupported reply.
- One coach job at a time, and none while a real summary or recording is running.
- Output is checked for grounding, not clinical accuracy. The user reviews every candidate.
- Not covered by automated native tests: availability gating during a live job and relaunch persistence are checked only by unit and component tests.
