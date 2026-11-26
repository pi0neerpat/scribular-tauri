# Recording & session spec — PR3 build plan (2026-11-25)

Spec: `plans/2026-11-16-recording-and-session-spec.md`, PR3 (R7, R10–R16; AS7, AS10–AS16).

Already on main: v9_note_redo migration, store undo/redo + copied_at, undo/redo/mark_note_copied
commands, prompt snapshot at Stop, deleted-style fallback, Coach snapshot check.

## Changes

1. **R10 Stop handoff** — RecordView calls `onStopped(id)` with the id from `stop_recording`; App
   navigates to History and opens that session. RecordView drops result/SummaryPanel/Timings/
   progress blocks. History row shows "Writing…" for finalizing too; session page shows progress.
2. **R11 tabs** — SummaryPanel (management) gets Note/Transcript/Audio tabs, Note first; header
   with date/duration, speakers, style. loadFailed copy + Try Again; Copy Transcript; no Regenerate
   or Delete buttons (delete via menu/context). View menu Cmd+1/2/3 enabled.
3. **R12 edit/rewrite/undo** — in-place textarea saved via update_session; picking a style rewrites
   at once (saving a dirty draft first); Undo line after rewrite/Coach until edit/copy/navigation;
   Edit menu Undo/Redo become shell items routed to text undo or note undo.
4. **R13 copy** — Copied is temporary; list checkmark from copiedAt; Copy Transcript never marks.
5. **R14 speakers** — names apply on Done; pending choices survive reopen; read-only when lapsed.
6. **R15 audio** — custom player (play/pause, scrub, ±15 s, speed); disabled tab with reason when
   no audio; new `get_session_segments` command for timed transcript lines, highlight + seek.
7. **R16 recovery** — per-session Recover page (explanation, Recover Recording, Delete); remove
   the App banner and sidebar aggregate line.
8. **AS7** — Rust tests: deleted style falls back; later global change leaves queued note alone.

Tests: vitest for each UI change, Rust tests for AS7/segments, uitest scenarios for AS10–AS14,
AS16 in the task dir.
