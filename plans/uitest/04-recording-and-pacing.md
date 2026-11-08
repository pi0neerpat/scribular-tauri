# Plan 4: uitest recording and pacing (scribular-tauri, Swift) -- written as a cwf spec

Run via the standard pattern: fresh clone `scribular-tauri-pNN` (next free number), `cwf init --reviewer cubic --harness codex --phase-harness review=claude --start`. Copy this file to `plans/uitest/04-recording-and-pacing.md` in the clone at kickoff. Scope: `tools/uitest/` only. Add flags/commands/tests without changing existing scenarios or assertions. The approved recording policy change removes the old always-on `runs/<ts>/recording.mp4`; existing scripts that consumed those per-run recordings must opt in with `--record`, and current UITest scenarios do not otherwise consume that path.

## Intent
Reviewers need a watchable recording of the happy path, not a log of fast clicks. Today every `uitest` run records `runs/<ts>/recording.mp4` at full speed (`Recorder.swift`, ffmpeg avfoundation 15 fps), eight were made for spec 06 and seven for spec 07, a later PR cited a recording file that did not exist, and an old ffmpeg was left running (spec 07, Patrick's prompt-0023).

## Requirements
- R1. `--record <name>` starts the recorder and writes `<record-dir>/<name>.mp4` plus `<name>.json` (scenario, app build time, step count, duration, final result, and per-step timestamps). **Without `--record` no ffmpeg process starts.** Failed steps still save `fail-N.png` and the label dump (existing behaviour). The approved behavior change removes the old always-on `runs/<ts>/recording.mp4`; existing scripts that consumed those per-run recordings must opt in with `--record`.
- R2. `--record-dir <dir>` or env `CWF_RECORDINGS_DIR`; default the primary checkout's `runs/` when the repo is a clone (derive from `git rev-parse --git-common-dir` is NOT valid for clones; use `git remote` -> search sibling `scribular-tauri` directory, else `./runs`).
- R3. `--pace <duration>` (default `2.5s` when `--record` is given, `0` otherwise): after every state-changing step (`click`, `type`, `key`, `close-window`, menu actions) sleep the pace; after an `expect` that follows a visible change sleep 1 s. Script command `pause 3s` for read time and `pace 2s` to change the pace inside a script. Explicit `wait` unchanged.
- R4. On finish hold 3 s on the last state before stopping ffmpeg. A failed recorded run renames to `<name>-FAILED.mp4`.
- R5. A stray recorder from a crashed run is stopped at the next `uitest` start (kill ffmpeg processes whose output path is under a uitest run/record dir and whose parent uitest is gone).
- R6. README and `CLAUDE.md` E2E section updated: recording policy (happy path or unresolvable issue only), naming `<NN>-<slug>-happy-path`, pacing.

## Acceptance scenarios
- AS1 (unit): flag parsing and pace scheduling (`PaceTests`) -- default pace with/without `--record`, `pause`/`pace` commands, no sleep when pace is 0.
- AS2 (unit): `--record` absent => recorder is never constructed (inject a fake).
- AS3 (real run, VM): `system-audio-smoke.uitest --record 99-smoke-happy-path` produces mp4+json; timestamps in the json show >= 2 s between consecutive state-changing UI actions; mp4 duration within 3 s of the json duration.
- AS4 (real run): the same scenario without `--record` leaves no ffmpeg process and no mp4.
- AS5 (real run): failing scenario with `--record x` leaves `x-FAILED.mp4`.
- AS6: all existing unit tests and scenario parse tests unchanged and green (`swift test --package-path tools/uitest`).

## Evidence method
AS1, AS2, AS6 `log/test`; AS3-AS5 `ui-script` with the json timestamps as proof. The PR's successful recording is `04-uitest-recording-controls.mp4`.

## Out of scope
Native/tray support (Plan 5), cwf changes (Plans 1-3).
