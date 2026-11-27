# Recording & session spec — PR4 (2026-11-26)

Parent spec: `plans/2026-11-16-recording-and-session-spec.md`, "PR4 (S–M, 2–3 days)": real Mac
AudioCapCLI/permission/playback walkthrough, privacy fault checks and integration fixes
(AS17, AS18). PR1–PR3 are merged on `main`.

## Scope

1. **AS18 (automated, `log/db/state`)** — Rust tests with fault injection for attach, pause,
   clipboard and store errors. Assert:
   - diagnostic output (logs, error strings surfaced to the UI/frontend error handler) contains
     only event/state/count metadata: no transcript text, note text, style prompts, speaker names
     or file paths with session content;
   - committed chunks and notes survive each injected fault;
   - model/Coach jobs in flight cannot block mic Start.
   Fix any leak or ordering bug the tests expose in the existing code (smallest fix, root cause).
2. **AS17 (manual-only, real macOS AudioCapCLI + permissions)** — prepare a short walkthrough
   checklist in `plans/evidence/recording-p4-as17.md` (steps, what to record: elapsed/duration,
   both meters, warning transition, final transcript, retained-audio playback). Run it only if the
   task environment has a real Mac with System Audio permission granted. Otherwise write the file
   with each step marked **UNVERIFIED (not run)** and the exact reason. Never claim a hardware
   check that was not run.
3. **Integration fixes** — fix bugs found while doing 1 and 2 (and only those). No new features.

## Out of scope

Tab-specific capture, new UI, any change to AS1–AS16 behavior except fixing a proven bug.

## Verification

- `cargo test` (new AS18 tests included) and `npx vitest` pass.
- uitest scenarios unaffected; add one only if an integration fix changes UI behavior.
- Evidence file lists each AS17 step as run or UNVERIFIED, and AS18 test names with results.

## Prior work

A cancelled earlier attempt left uncommitted AS18 edits in
`/Users/admin/Projects/Scribular/recording-p4-dirty-edits-2026-11-26.patch` (applies cleanly to
`c910c71`: clipboard `copy_with` fault test in `commands.rs`, plus `queue_coach_tests.rs`,
`tests/session_job.rs`, `src/lib/types/Session.ts`). Read it first and reuse what is correct.
