# Cleanup report: feat-recording-p3 (PR 64)

## Summary
PR 64 (part 3 of `plans/2026-11-16-recording-and-session-spec.md`: AS7, AS10-AS16) merged at `c910c7114149f5fc3578a91a1fb072409aed772c`; reviewed head `1c1e16e`. The run took 41 agent sessions and 11 head changes, and ended in `cleanup`. Two review blocks (round-3 non-convergence, 2026-11-26 08:05 and 2026-11-27 01:02) and the last redundant verify loop were the main costs. The final stretch (prompts 0031-0040) was a cwf-hook artefact: product feedback sent to the *supervising chat* was injected into the task and restarted build+verify for a scenario-only change.
Automatic mode: step 8 skipped, no skill edits made. Docs subagent not run (see "Not done").

## Metrics
| Metric | Value |
|---|---|
| Active / human-wait / CI-wait | 44,165 s (12.3 h) / 49,683 s (13.8 h) / 4,505 s (1.3 h) |
| Review rounds / fix pushes | 0 recorded (`review_rounds`) / 11 |
| Compactions / resumes | 8 / 9 |
| Interventions | 41 (all `classification: null` except one `pause`) |
| Premature stops / stop blocks / false ready | 0 / 0 / 0 |
| needs_human events | 6 (2 non-convergence, 1 reviewer silent, 1 product review ready, 1 user stop, 1 other) |
| Demos | 1 happy-path (+1 earlier file), stale 0 |
| Terminal state | cleanup (PR merged) |
| Phase settings | `~/.local/state/clauffice/workflows/phase-settings.json`; `phase_settings_drift`: [] |
| Models | build opus x15, verify/review/cleanup sonnet (26 sessions) |

Tokens (`session_tokens.py`; all 41 sessions known): build 41.1 M, verify 36.4 M, review 3.9 M, cleanup 0.13 M; grand total 81.6 M (almost all cached reads). Claude only; no Codex usage.

## Process review
**Value per session.** 15 build / 15 verify / 10 review sessions. Review sessions were 0.5-1.8 min each and mostly relayed Cubic output (cheap). Overlap: on head `1c1e16e` the last build (2.97 min, events ~578-584) and verify (12.3 min) re-ran work for a change that touched only task-dir scenarios; the user stopped it (`needs_human` line 601, prompt-0038). Verify at 113 min (2026-11-26 20:57-22:50) and 49-48 min later ones dominate wall time; AS12/AS14 failed once on `0f17578` before passing.
**Communication.** The final handoff (`5fc71989...ingested.json`) says the committed `recording-pr3-as11*/as15-playhead.uitest` "already run close-window" and "no repo change". The checkout shows otherwise: those three committed files have uncommitted `close-window` additions (`git diff`), so the repo *did* change and the fix is not in `main`. Dropped item: the redo of the demo recording (prompt-0031) never happened; the stop interrupted it.
**Time.** Idle gaps of 610 min (verify->build, after the first non-convergence block) and 168 min (after the second) were waits for Patrick; 48.5 min review->build before the final feedback. Stalls: reviewer silent (`needs_human` line 325, `reviewer_wait_expired` x3, 29 `reviewer_network_error`).
**Environment gates (infrastructure).** (a) Verify used the on-VM llama-server (1.75 GB RSS, prompt-0011/0012/0024/0025) until PR #65 made uitest use the host server; fix is merged in main, so no further note needed except the merge-before-verify instruction (prompt-0025). (b) Cubic did not respond (line 325).
**Parallel implementation.** Spec did not recommend splitting; build ran as single sessions.
**PR hygiene.** One comment (the review package), no duplicates. PR body is cubic-generated and starts with prose, not a filename. The review package has a "Test it yourself" section, no "Proof of success" title and no mermaid journey diagram; recording is a bare task-dir path.
**Demo evidence.** One happy-path demo named per spec. The demo shows Settings covering the main window (prompt-0031, product feedback), so it needs a redo; scenario fix exists only as uncommitted edits (see above). I did not verify lead-in length.
**Package.** Fails the "Proof of success" + mermaid requirement; "Human Verification Needed" naming not used.

## Classified items
| Item | Class | Evidence |
|---|---|---|
| 0001-0005, 0008, 0016-0018, 0029-0030 (cwf design / context-diet planning, deploy removal) | product_uncertainty | prompts name decisions only Patrick owns |
| 0006, 0010, 0013, 0019, 0023 (subagent hand-backs) | task-notification, filtered | `[Subagent hand-back]` |
| 0007 "verify looks stuck", 0020/0022 "stuck on review / blocked" | infrastructure | reviewer silent, `needs_human` lines 204, 325 |
| 0009 operator procedure, 0014/0015/0032/0033 harness choice | product_uncertainty | harness selection |
| 0011/0012/0024/0025 host llm | infrastructure (fixed in PR #65) | prompts cite 1.75 GB RSS |
| 0026-0028 cleanup stall / `_deploy_self` | bad_instruction | removed per prompt-0030 |
| 0031 settings window covers demo | model_error | AS11 scenario never closed Settings |
| 0034-0038 redundant verify loop, 0036 "I never sent a terminal message", 0037 | missing_capability | hook injected chat feedback into the task (line 601 reason) |
| 0039-0040 hook question, move to cleanup | product_uncertainty | |
| 0041 cleanup prompt | task-notification, filtered | |
Round-3 non-convergence (line 492): scope growth (~80 new lines of new uitest commands after Cubic approval), not oscillation (see transition at line 501): `model_error`.

## Proposed changes (not applied)
1. **File** `workflows/coding/` prompt-injection hook (supervisor intervention ingestion). **Change:** do not turn a chat message into a task `intervention` unless it is sent in the task's own tmux session or addressed with the task id. **Evidence:** prompts 0034-0038, events 578-601; 5.5 min of redundant build+verify and a user-forced block.
2. **File** `workflows/coding/skills/coding-task/templates/review-package.md`. **Change:** title the recording block "Proof of success" with a mermaid journey diagram; rename "Test it yourself" to "Human Verification Needed", numbered one-action steps, only human-only items. **Evidence:** PR 64 comment 1 has neither.
3. **File** `workflows/coding/skills/coding-task/SKILL.md` (build/verify). **Change:** do not edit repo files during `product_feedback` handling and still report "no repo change"; handoff must match `git status`. **Evidence:** handoff `5fc71989...` vs `git diff` (three uncommitted scenario edits).
4. **File** `workflows/coding/skills/coding-task/SKILL.md` (build). **Change:** after reviewer approval, forbid new uitest commands in review-fix rounds. **Evidence:** line 501 transition.
5. **File** `workflows/coding/skills/cleanup/SKILL.md` step 4/final step 5. **Change:** if the worktree equals the user's primary checkout (`primary_checkout: null`, dirty, untracked user files), do not delete it. **Evidence:** `cwf status` worktree is `~/Projects/Scribular/scribular-tauri`.

## Not done / follow-up for Patrick
- Demo redo for PR 64; commit the three `close-window` scenario edits (currently uncommitted in the checkout).
- Docs for the shipped feature (step 6 docs subagent) were not run; report PR only.
- Clone not deleted: the task worktree is Patrick's primary checkout with uncommitted work and untracked plans; `~/cargo-target/scribular-tauri` left for the supervisor.
- Next part: `spec.part` is 3; the spec does not state a part 4 here, so no `cwf init` command is given.
