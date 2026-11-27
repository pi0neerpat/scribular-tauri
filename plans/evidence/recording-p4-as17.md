# Recording PR4 evidence: AS17 walkthrough and AS18 tests (2026-11-26)

Spec: `plans/2026-11-26-recording-p4-spec.md`. Parent: `plans/2026-11-16-recording-and-session-spec.md` (AS17, AS18).

## AS17: real Mac AudioCapCLI and permission walkthrough

AS17 is tagged `manual-only (real macOS AudioCapCLI and permission behavior)`. The build session ran in
a Tart VM (`Apple Virtual Machine 1`, Apple M2 (Virtual), macOS 26.2). The VM has no real call app
audio path and no System Audio Recording permission granted by a person, so no step below was run.
Every step is **UNVERIFIED (not run)**: this task environment is not a real Mac with System Audio
permission granted. Patrick runs the checklist on a real Mac before release.

Setup: a real Mac with Screen & System Audio Recording permission granted to Scribular, a release or
debug build of this branch, a call app (e.g. Zoom or FaceTime) open but silent, and one microphone
with a person speaking.

| # | Step | Record | Result |
| --- | --- | --- | --- |
| 1 | Choose Mic + App audio and select the silent call app. Click Start. | Recording starts from the mic at once; elapsed timer starts at 0:00. | UNVERIFIED (not run): not a real Mac with System Audio permission |
| 2 | Speak for about 10 s with the app silent. | Mic meter moves; app meter stays flat; elapsed advances. | UNVERIFIED (not run): not a real Mac with System Audio permission |
| 3 | Wait for the no-app-audio warning. | Time and wording of the warning transition (silent app → warning shown). | UNVERIFIED (not run): not a real Mac with System Audio permission |
| 4 | Click Pause, wait about 5 s, click Resume. | Elapsed freezes while paused and resumes from the same value; meters stop and restart. | UNVERIFIED (not run): not a real Mac with System Audio permission |
| 5 | Make the call app play sound (e.g. a test call or media in the app) while speaking. | App meter moves; warning clears (record the transition); AudioCapCLI attaches late without restarting the mic. If it cannot attach late, record the exact failure text from the log. | UNVERIFIED (not run): not a real Mac with System Audio permission |
| 6 | Click Stop. | Final duration equals the elapsed shown at Stop, excluding the pause. | UNVERIFIED (not run): not a real Mac with System Audio permission |
| 7 | Wait for the session to finish. | Final transcript contains both the spoken mic words and the app audio words. | UNVERIFIED (not run): not a real Mac with System Audio permission |
| 8 | Play the retained audio in the session view. | Playback runs locally from retained audio; playhead and duration match step 6. | UNVERIFIED (not run): not a real Mac with System Audio permission |

## AS18: automated fault-injection tests

Run on the build head with `cargo test --features fake-engines` (src-tauri). All pass.

| Test | Covers | Result |
| --- | --- | --- |
| `commands::tests::as18_clipboard_faults_report_no_note_text` | Clipboard fault (missing program, failing program): the error names the step only, no note text or paths. | pass |
| `tests/session_job.rs::as18_attach_pause_and_store_faults_keep_notes_and_log_only_metadata` | Attach fault (app not playing), pause fault (chunk insert fails), store faults (note edit, copy mark): log and surfaced errors hold no app name, title, note, transcript or DB path; committed chunk and earlier note survive; session recovers. | pass |
| `tests/session_job.rs::capture_faults_log_metadata_and_keep_committed_audio` | Existing capture fault test, now serialised with the new test on the shared diagnostics log. | pass |
| `jobs::queue::coach_tests::as18_in_flight_session_coach_cannot_block_mic_start` | A Coach generation stuck in the model does not block mic Start; Start cancels it. | pass |
| `jobs::queue::coach_tests::as18_summary_model_load_in_flight_cannot_block_mic_start` | A practice job waiting on a slow summary model load does not block mic Start; Start cancels it. | pass |

The tests exposed no leak or ordering bug, so no product code changed beyond making the clipboard
command testable (`copy_text` now calls `copy_with`).
