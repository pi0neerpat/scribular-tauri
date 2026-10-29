# U10 defect repair report

Branch: `feat/u10-first-run`
Base U10 commits preserved: `a17e19c`, `c4e1936`
Repair commits: `3685dcc` (durable queue/state) and `6218600` (first-run UI)

## What changed

- `awaitingModel` no longer blocks the Record view. A second recording can start while an earlier transcript waits for the summary model.
- Summary queue rows now remain in SQLite while a job runs and are deleted only when it finishes or is cancelled. Launch recovery returns queued `Finalizing` and `Summarizing` sessions to `AwaitingModel` without changing FIFO order.
- Added migration v3 to keep a queued re-summary's original status. Cancelling a queued re-summary restores a prior `Done`, `Failed`, or `Cancelled` state; cancelling a first summary marks it `Cancelled`.
- Re-summary from a finished state may legally enter `AwaitingModel`.
- First run now offers System Settings actions for microphone and system-audio permissions. The existing denial path maps microphone denial to the same action. Failed summary-model downloads can be retried; an absent model is shown as not started.
- No dependencies or generated IPC type changes.

## Six required scenarios → tests

1. **Stop while the summary model is unavailable; transcript is immediately viewable and summary resumes:** `session_job::stopping_while_the_llm_is_downloading_saves_transcript_and_resumes`; `awaiting_model::transcript_waits_at_download_progress_and_summarizes_when_ready`. The fake loader represents the reported 60% point by `ready = false`; it does not simulate a byte-accurate 60% download.
2. **Awaiting session is shown as waiting with transcript visible:** `first-run.test.ts` — “shows an AwaitingModel session as waiting while keeping its transcript viewable”.
3. **Two sessions summarize in stop order:** `awaiting_model::multiple_waiting_sessions_summarize_in_stop_order`.
4. **Queued sessions survive relaunch and retain order:** `awaiting_model::waiting_sessions_resume_from_persistent_queue_after_relaunch` now covers two sessions; `awaiting_model::restart_recovers_a_queued_summarizing_session_and_keeps_fifo_order` covers a crash after work was popped and a queued `Finalizing` session.
5. **Recording availability follows speech readiness and ignores prior AwaitingModel work:** `first-run.test.ts` — “keeps Start recording disabled until the speech model is ready”, “enables Start recording once the speech model is ready”, and “keeps recording available while a prior session awaits the summary model”.
6. **Denied microphone permission exposes System Settings:** `first-run.test.ts` — “shows the System Settings action when microphone permission is denied” exercises `deniedPrivacy` mapping and renders `PermissionNotice`; “shows first-run actions for both macOS permission panes” covers the onboarding actions.

Additional regression: `awaiting_model::cancelling_a_queued_resummary_keeps_the_completed_session_and_drops_the_row` verifies cancellation does not resurrect a queued job or discard the completed result.

## Verification

Commands were run with `CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0`.

| Command | Result |
|---|---|
| `cargo test` | Exit 0; 80 passed, 4 ignored (real-model tests) |
| `cargo test --features fake-engines` | Exit 0; 114 passed, 0 failed |
| `cargo clippy --all-targets --features fake-engines` | Exit 0; no warnings from changed files. Three existing warnings remain in `jobs/session.rs`, `stt/stream.rs`, and `audio/ring.rs`. |
| `cargo test -- --ignored` | Final full rerun exit 0; 4 real-model tests passed (2 llama, 2 whisper). The first run had one transient whisper cancellation latency failure (1.21 s vs the 1 s assertion); the targeted whisper rerun and full rerun both passed. |
| `npx svelte-check` | Exit 0; 0 errors, 0 warnings |
| `npx vitest run` | Exit 0; 14 tests passed across 3 files |
| `git diff --exit-code src/lib/types` | Exit 0; no generated IPC type changes |
| `git diff --check` | Exit 0 |

`npm ci` was skipped because `node_modules` was present. No new package was added.
After verification, `cargo clean` removed the generated 1.7 GiB `src-tauri/target`; free disk is 2.9 GiB (97% used).

## Deferred / pending

- The summary worker still checks `SummarizerLoader::ready()` on its 200 ms wait cycle while queued work is blocked. The plan’s U4 note (around line 391) calls for a readiness watch channel; the loader trait currently exposes only a boolean `ready()` and no notification subscription. Replacing that contract spans the loader, model readiness, and fake engine; this repair leaves it as a documented follow-up rather than changing unrelated interfaces.
- The first-run permission and readiness UI is covered with the existing server-rendered component test tooling. It does not automate a native macOS permission prompt or click through System Settings.
- Manual fresh-macOS-user check remains pending for the developer, including granting permissions and recording a second session while the summary model downloads.

No dependency blocker. U9 remains accepted as passed per Patrick; no U9 evidence or measurements were changed or added.

## Follow-up repair (2026-10-29)

This section records the post-review repair; the earlier report above is preserved. Existing implementation commits remain unchanged.

### Changes

- `FirstRun` now renders failed speech-model errors and an explicit Retry action that calls the existing `downloadModels`; progress still shows download percentage. `SpeechModelStatus` is a small renderable component for meaningful failed-state coverage.
- `regenerate` now reads session state while holding the queue mutex, skips the debug transition assertion for a same-state `AwaitingModel` update, and keeps worker cleanup serialized with requeue. A job requeued after summary completion retains its FIFO row. Existing SQLite upsert behavior updates only `prompt_id`, retaining the original `restore_status` across repeated updates.
- Normal `JobQueue` shutdown now leaves an interrupted active job in its durable row and returns its status to `Summarizing`, which launch recovery moves back to `AwaitingModel`. Pending queue cancellation is serialized with queue mutation.
- Added `awaiting_model::updating_a_waiting_job_keeps_its_original_restore_status`, `awaiting_model::requeue_during_completion_keeps_the_new_durable_row`, and `awaiting_model::dropping_during_a_summary_keeps_the_job_for_relaunch`; added a speech failure/retry render test.
- No dependencies, IPC changes, U9 edits, or model downloads.

### Review observations and scope

- The failed speech download, repeated AwaitingModel queue update, and completion/requeue race were concrete defects and are repaired above.
- The active-job quit cancellation was also concrete against KTD7's persistent FIFO queue requirement, so it is repaired and regression-tested across reopening the store.
- The missing way to reopen an earlier session and the lack of a UI cancel action are session-history/navigation concerns. R6 and the TAURI_MVP architecture specify session browsing/detail, but U10's scope and six scenarios cover first-run readiness, record-while-summary-downloads, and immediate transcript availability (R10–R12), not adding session navigation. The existing saved session remains in SQLite and U10 queue work continues; R6 UI navigation remains a separate requirement gap. No unrelated navigation UI was added in this repair.
- The prior review's frontend tests are server-rendered; the new failure test verifies visible error and retry affordance, not a browser click or native download.

### Verification after repair

All Rust commands used `CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0`.

| Command | Result |
|---|---|
| `cargo test` | Exit 0; 80 passed, 4 ignored (real-model tests) |
| `cargo test --features fake-engines` | Exit 0; 117 passed, 0 failed |
| `cargo clippy --all-targets --features fake-engines` | Exit 0; 3 warnings remain in unchanged `src/jobs/session.rs`, `src/stt/stream.rs`, and `src/audio/ring.rs`; no warnings from changed files |
| `cargo test -- --ignored` | Exit 0; 4 passed (2 llama, 2 whisper), using already-installed models |
| `npx svelte-check` | Exit 0; 0 errors, 0 warnings |
| `npx vitest run` | Exit 0; 15 passed across 3 files |
| `git diff --exit-code src/lib/types` | Exit 0; no generated IPC type changes |
| `cargo fmt --check` / `git diff --check` | Exit 0 |

`npm ci` was unnecessary because `node_modules` exists. A first targeted Rust test compile caught `let_underscore_lock` in the new test harness; it was fixed, and the rerun passed. The added shutdown regression initially asserted before reopening the store; it was corrected to exercise actual launch recovery and then passed. These transient failures were test-harness issues, not product test failures.

### Still pending

- Manual fresh macOS user check remains pending: grant mic/system-audio permissions, wait through speech setup, record, and record again while the summary model downloads. No readiness or U10 sign-off is claimed pending that manual check and independent review.
- U9 remains passed per Patrick; no measurements or `m5b-check.md` were changed.
