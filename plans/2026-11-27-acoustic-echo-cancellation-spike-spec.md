# Acoustic echo cancellation spike — cwf task spec

Date: 2026-11-27. Plan: [2026-11-27-acoustic-echo-cancellation-spike-plan.md](2026-11-27-acoustic-echo-cancellation-spike-plan.md).
Follow that plan's steps, time-box (10 h, hard stop 12 h), fixture and success criteria.

## Deliverable

- The PR is docs-only: the plan plus `plans/2026-11-27-acoustic-echo-cancellation-spike-results.md` (decision note).
- No product code, dependency or `Cargo.toml`/`Cargo.lock` change is committed. The offline harness, live-path
  prototype and recorded WAVs live under the cwf task directory (`<task_dir>/spike/`), not in the repo. The decision
  note cites their paths and the exact commands to rerun them.
- The decision note records: the fixture used; baseline duplicate-line rate; per crate and per delay setting the
  duplicate rate, ERLE (from t = 0 and after 3 s), CPU, dropped blocks and added latency; the Pass / Partial / Fail
  verdict from the plan's "Decision the spike produces"; the named crate if Pass; answers to the plan's open items.

## Acceptance scenarios

- AS1 (automated): baseline. Run A (fixture through speakers via App audio + `play`, "On a call", no speech) three
  times on this Mac; the note reports the baseline duplicate-line rate and residual level from those runs.
- AS2 (automated): offline harness on the AS1 recordings for `sonora` and `aec3` at stream delays 0/50/100/150/250 ms;
  the note has a results table and the cleaned-mic whisper duplicate count per cell.
- AS3 (automated): only if AS2 meets criteria 1–2, the live-path prototype on the mic `live` feed; the note reports CPU,
  `ring.rs` drop counters and added latency with and without AEC, and confirms stored chunks and the mixed WAV are
  byte-identical to a control run.
- AS4 (manual-only (needs a person reading the script aloud over the fixture)): run B double-talk WER check.
  Record as unverified for Patrick with exact steps.
- AS5 (manual-only (needs a Bluetooth speaker paired by hand)): reference-quality check on a Bluetooth speaker.
- AS6 (automated): `git diff origin/main --stat` on the PR shows only `plans/` files.
- If both crates fail AS2, the 1 h VPIO probe from plan step 6 runs and its result is in the note.
