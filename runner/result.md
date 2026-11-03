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


## Focused continuation (2026-10-30)

Starting HEAD: `bcbe66d`, branch `feat/u10-first-run`. Local implementation commits: `067fabf` (actionable speech retry and readiness coverage), `9929e7e` (queue identity and cancellation persistence). All earlier commits are preserved. Work stayed in this checkout; no push, PR, merge, delegation, dependency changes, production model download, or host permission/account changes. U9 is accepted per Patrick; `plans/evidence/m5b-check.md`, STT thresholds, and `audio/ring.rs` were not changed.

### Findings verified and repaired

1. **Speech retry during LLM download:** the single `ensure_all` guard really did reject Retry throughout the LLM download. Guards are now per manifest entry. Calls still visit speech first and stop at an already-active model, preventing concurrent callers from starting the LLM ahead of speech. A failed speech model can retry beside an active LLM download. Work is bounded to one request per model (at most two with the pinned manifest); no duplicate writers touch a model's partial file. Existing Range resume, verification, error reporting, and guard release on future drop remain intact.
2. **New requeue versus pending-job update:** `regenerate` determines whether the job is already in the pending FIFO while holding the queue mutex. `queue_status` preserves the prompt-update behavior for that same pending job. A new job atomically replaces any finishing row before insertion, acquiring a new tail position and the current restore status. No schema migration is required. Cancelling a requeue of a completed summary restores `Done` and preserves its text; restart order matches the in-memory order.
3. **Explicit cancel versus shutdown:** the running job records explicit user cancellation separately. Shutdown cannot restore it as interrupted work or overwrite a newly pending job. Accepting explicit cancellation also atomically removes the durable row and marks in-flight status cancelled, before signalling the worker; errors return false rather than falsely accepting cancellation. This covers the gap between popping a job and entering `Summarizing`, while leaving terminal results unchanged. Regeneration is rejected until the cancelled worker releases the session, preventing it from overwriting a new job.

The review's concern about relying on `JobQueue::drop` was supported by local dependency source: Tauri 2.12.1 `cleanup_before_exit` clears resource tables, not managed queue state; Tao 0.37.1's macOS `EventLoop::run` calls `process::exit`. Consequently, cancellation persistence is tested before worker cleanup or Drop. Genuine shutdown interruption still retains its durable row, and existing crash recovery still resumes it. No application lifecycle rewrite was added.

Changed production files: `src-tauri/src/models/mod.rs`, `src-tauri/src/jobs/queue.rs`, and `src-tauri/src/store/sessions.rs`. Regression coverage is in `tests/models_download.rs`, `tests/awaiting_model.rs`, and `tests/session_job.rs`.

### New behavioral regressions

- `models_download::speech_retry_finishes_while_llm_download_is_still_running`: local TCP server truncates speech, serves an active LLM response, and receives eight concurrent retry calls. Speech reaches Ready before LLM completion; exactly one speech Range resume and one LLM request occur; both verified files match their fixture bytes.
- Strengthened `models_download::speech_model_downloads_before_the_llm`: a concurrent ensure call cannot start the LLM while speech is active.
- `awaiting_model::cancelling_a_requeue_during_completion_restores_the_new_done_status`: cancels while the old worker's completion callback is held; reopens SQLite and checks Done plus retained summary.
- `awaiting_model::requeue_during_completion_moves_to_the_tail_across_restart`: another waiting session runs before the new requeue after an actual store reopen.
- `awaiting_model::updating_the_same_pending_job_preserves_fifo_across_restart`: a changed prompt retains the existing position and is used after reopening. Existing `updating_a_waiting_job_keeps_its_original_restore_status` still covers restore status.
- `awaiting_model::explicit_cancel_then_quit_does_not_resume_on_reopen`: explicit cancellation followed by queue shutdown and reopening produces no resumed work.
- `awaiting_model::explicit_cancel_is_durable_before_the_worker_can_finish`: reopens the store with the worker held in a callback, proving persistence does not depend on Drop; a requeue cannot race the cancelled worker.
- `awaiting_model::durable_running_cancellation_covers_the_gap_before_summarizing`: persisted cancellation covers Finalizing, AwaitingModel, and Summarizing, while preserving Done/Failed/Cancelled terminal states.
- `session_job::first_run_refuses_recording_until_speech_is_ready_without_creating_a_session`: unavailable speech blocks the backend before capture opens or a session is created.

The original regressions were observed failing before their fixes: retry remained Failed; requeue cancellation restored Cancelled instead of Done; restart order was reversed; explicit cancel was recovered as AwaitingModel or summarized again to Done. The pre-Summarizing cancellation boundary also failed before correction (Finalizing reopened as Failed). The first cancel/quit test exposed a test-helper condition variable shared by two mutexes; it now uses a separate pause notification, and the corrected test reproduced the product failure. Targeted tests then passed. The plain suite was rerun after the final cancellation boundary change. The previous real-model attempt stopped at the usage limit without a final exit status: its chunk-budget test passed, the seven-prompt test was still running, and the STT tests had not started. That incomplete attempt is not counted as a completed suite; the resumed run is recorded below.

### Six original acceptance scenarios → current evidence

| # | Required scenario | Tests and limits |
|---|---|---|
| 1 | Stop with LLM at 60%: viewable saved transcript, AwaitingModel, automatic summary after readiness | `session_job::stopping_while_the_llm_is_downloading_saves_transcript_and_resumes`; `awaiting_model::transcript_waits_at_download_progress_and_summarizes_when_ready`. Both pass. The fake loader represents 60% as not-ready; it is not a measured or byte-accurate 60% model download. |
| 2 | Summary panel identifies waiting and retains the transcript | `first-run.test.ts`: “shows an AwaitingModel session as waiting while keeping its transcript viewable” — passes; server-rendered component test. |
| 3 | Waiting sessions summarize in stop order | `awaiting_model::multiple_waiting_sessions_summarize_in_stop_order` — passes. New requeue-tail and same-pending-update tests additionally protect FIFO across restart. |
| 4 | Pending sessions resume after relaunch | `awaiting_model::waiting_sessions_resume_from_persistent_queue_after_relaunch`, `restart_recovers_a_queued_summarizing_session_and_keeps_fifo_order`, and `dropping_during_a_summary_keeps_the_job_for_relaunch` — pass. Explicitly cancelled work is separately tested not to resume. |
| 5 | Record disabled until speech is Ready; available while LLM downloads | `first-run.test.ts`: “keeps Start recording disabled until the speech model is ready”, “enables Start recording once the speech model is ready”, and “keeps recording available while a prior session awaits the summary model” — pass. New `session_job::first_run_refuses_recording_until_speech_is_ready_without_creating_a_session` also verifies the backend guard. |
| 6 | Denied mic exposes Open System Settings | `first-run.test.ts`: “shows the System Settings action when microphone permission is denied” and “shows first-run actions for both macOS permission panes” — pass. Backend `session_job::a_refused_source_leaves_no_session` and `a_refused_system_capture_names_system_audio` also pass. These are separate automated checks, not a native Start → permission prompt → denial → settings click-through. |

### Verification on the final code

The passing results below are retained from the previous run on the code committed as `9929e7e`; those suites were not rerun during this docs-only continuation. The ignored real-model run and final documentation checks are identified separately below.

Rust settings throughout: `CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0`. No new dependencies. `node_modules` already existed, so `npm ci` was unnecessary.

| Command | Exit / actual result |
|---|---|
| `cargo test` | 0; **81 passed, 4 ignored**, 0 failed |
| `cargo test --features fake-engines` | 0; **125 passed**, 0 failed; awaiting-model tests 15/15, download tests 6/6, recording-flow tests 19/19 |
| `cargo clippy --all-targets --features fake-engines` | 0; no warnings in changed files. Three existing warnings: `jobs/session.rs:141` unnecessary_map_or, `stt/stream.rs:145` should_implement_trait, `audio/ring.rs:255` manual_repeat_n. |
| `cargo test -- --ignored` | 0 on the resumed run; **4 passed, 0 failed, 0 ignored**, 81 filtered out; 2 llama and 2 Whisper tests using installed models |
| `npx svelte-check` | 0; 0 errors, 0 warnings |
| `npx vitest run` | 0; 15/15 tests, 3/3 files |
| `cargo fmt --check` | 0 |
| `git diff --check` | 0 |
| `git diff --exit-code src/lib/types` | 0; regenerated IPC types unchanged; command/event wrapper checks pass in Rust and frontend suites |

The resumed run started at `9929e7e` with 1.3 GiB free. Before building, `cargo clean --manifest-path src-tauri/Cargo.toml` removed 1.7 GiB of prior-run build artifacts, and the authorized `runner/u10-real-tmp` directory was removed. `df -k .` then measured **2,913,052 KiB available (2.8 GiB rounded)**, enough to attempt the build. Both installed model files were present; no model was downloaded or deleted.

Exactly one resumed `cargo test -- --ignored` invocation ran from `src-tauri` with the four Rust settings above and `TMPDIR=/Users/admin/Projects/Scribular/scribular-tauri-u10/runner/u10-real-tmp`. Build completed in 4m 42s. Actual test outcomes:

| Test | Result |
|---|---|
| `llama::fifty_minute_benchmark_splits_into_at_most_three_chunks` | Passed |
| `llama::every_prompt_summarizes_thirty_seconds` | Passed; all seven prompt templates completed |
| `stt::whisper::cancelling_partway_returns_within_a_second` | Passed against the unchanged `< 1 second` assertion |
| `stt::whisper::fixture_transcribes_with_ordered_timestamps` | Passed |

The llama test binary completed in 171.70s and STT in 4.04s; these are binary durations, not individual cancellation measurements. Successful cancellation does not print latency, so no exact latency is claimed. The prior independent review measured **1.135181s against the 1s limit**; that timing failure remains separate evidence and is not considered fixed by this passing sample. No threshold or STT code was changed and no targeted timing rerun was performed. Raw output remains untracked in `runner/u10-real-models-resumed.log`; the previous incomplete log is also retained untracked.

### First-run verification attempt and remaining checks

Available automation exercised local HTTP download failure/retry, real SQLite reopen/recovery, fake recording/readiness and denied-source behavior, and server-rendered first-run components. Tool discovery found no desktop-control or browser-control tools in this session, and the installed npm dependencies provide no browser automation driver. No real-app/native click-through or fresh-user verification is claimed.

The exact missing capability is a controllable native macOS GUI session with a fresh user's untouched microphone and Screen/System Audio Recording TCC permission state, plus an empty model store/controlled partial LLM setup. This user's models are already installed. Creating/resetting an account or changing host permission settings was explicitly excluded, and no such action was taken. A developer still needs to perform launch → grant/deny permissions → speech setup → first recording, including failed speech download Retry while the LLM downloads, then Stop with LLM incomplete and a second recording during the wait.

R6 session-history navigation remains an unrelated product gap: the UI cannot reopen/cancel an earlier waiting session through history. No navigation work was added. The existing 200 ms loader-readiness polling remains as recorded in the prior report; no loader API redesign was included.

No dependencies were added. Native fresh-user verification and independent review remain outstanding; **this report does not claim U10 readiness or sign-off**.

Final cleanup: `cargo clean --manifest-path src-tauri/Cargo.toml` exited 0 and removed 4,981 files / 1.3 GiB from the resumed build. The run-owned `runner/u10-real-tmp` directory was removed; assertions confirmed both it and `src-tauri/target` are absent. Final `df -k .` measured **2,907,536 KiB available (2.8 GiB rounded)**. Models and other files were not deleted. The command runner rejected the initial `rm -rf` cleanup syntax before execution; cleanup used Cargo's supported clean command and a path-checked removal of the authorized temporary directory.

Final documentation checks: `git diff --check` and `git diff --exit-code 9929e7e -- . ':(exclude)runner/result.md'` exited 0, confirming clean patch formatting and no other tracked changes. Only `runner/result.md` is included in the U10 docs commit; raw logs stay untracked. Already-passing suites and formatting/type generation checks were not rerun because code did not change. No push, PR, or merge was performed.
