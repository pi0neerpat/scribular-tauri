# Cleanup report: 20261125-feat-uitest-doctor-no-prompt

**Outcome:** merged. PR #60 merged 2026-11-25T03:08Z, merge commit b7f534a97d3eb57945b6d35cc7492a17783d8ef2. Added `uitest doctor --no-prompt`, tmux grant-holder naming, and debug-build docs. Cubic approved head e92c663; no outstanding findings. Single review round, no false-ready, no premature stop.

## Metrics
| Metric | Value |
|---|---|
| Active / human-wait / CI-wait | 1591 s / 2045 s / 559 s |
| Review rounds / fix pushes | 1 / 2 |
| Compactions / resumes / premature stops / stop blocks / false-ready | 0 / 0 / 0 / 0 / 0 |
| Interventions | 1 (the cleanup prompt itself, a task prompt: filtered, not an intervention) |
| Tokens (estimated for Codex) | 7.33M total: build 4.04M, verify 2.56M, review 0.65M, cleanup 0.08M |
| Models | build/verify/review: codex gpt-6.2-sol; cleanup: claude sonnet |

## Process review
- **Value per session.** 8 sessions: build, verify, verify, review, build, verify, review, cleanup. The first verify (2.0 min, 02:11) ended and a second verify started 0.72 min later (4.9 min) on the same head 4a3b54d: overlapping rounds. The final verify re-ran doctor-tmux, swift test and bundle check on e92c663; needed because head changed. Both review sessions were useful (first led to fix commit e92c663).
- **Communication.** The operator note about launchd MISSING grants was needed after an earlier needs_human (events: the first outside-launchd verify recorded `fail` on 4a3b54d, then `pass` on e92c663 once the note defined exit 1 as expected). Verify before the note treated the expected exit 1 as a failure. No dropped findings or unverified items found.
- **Time.** Build 10.5 min; verify 2.0, 4.9, 3.3; review 1.4, 2.3. Gaps: 4.1 min verify→review, 1.5 min verify→review, 0.7 min verify→verify, and 34.3 min review→cleanup (human/merge wait; PR merged 03:08, cleanup started 03:09). Environment gate: launchd context has no Accessibility/Screen Recording grant (`infrastructure`); evidence: logs/verify-doctor-outside.log.
- **Tokens.** See table. Codex counts are estimates from token_count deltas; cleanup session transcript usage is partial (0.2 min, still running).
- **Parallel implementation.** Spec did not recommend splitting; one agent per phase.
- **PR hygiene.** One review-package comment, no duplicates. The PR body ends with a bare local recording path line ("Recording: ~/.local/...mp4"), not a link, and at the bottom, not the top. Evidence logs exist. 
- **Demo evidence.** One happy-path recording (12.0 s, terminal replay of both doctor outputs), no dead lead-in at that length, no environment gate shown. Not paced for a reviewer; unknown whether pace was set.
- **Product review package.** Fails two template requirements: the block is titled "Recording", not "Proof of success", and it has no mermaid user-journey diagram. Its "Test it yourself" section is titled that way instead of "Human Verification Needed" (content "None." is correct).

## Classified items
| Item | Class | Evidence |
|---|---|---|
| Earlier needs_human: launchd doctor exit 1 | `infrastructure` / `product_uncertainty` (operator ruled it expected) | watcher-config.json operator_note; verification[] fail then pass |
| Back-to-back verify sessions on 4a3b54d | `bad_instruction` (verify had no rule that expected-negative checks are not failures) | events.jsonl session_end 02:13:43 then session_start 02:14:26 |
| Review package title/diagram/section name | `model_error` or template gap: unknown which | PR #60 comment IC_kwDOVQZtns8AAAABqjoMvA |

## Proposed changes (not applied; automatic mode)
1. **Target:** `workflows/coding/skills/coding-task/templates/review-package.md`. **Change:** make the recording block "Proof of success" with a mermaid user-journey diagram and rename "Test it yourself" to "Human Verification Needed" with numbered one-action steps. **Evidence:** PR #60 package lacks all three (only if the template does not already say so; not checked, template not read this session).
2. **Target:** `workflows/coding/skills/coding-task/` verify instructions. **Change:** when a spec says a check must fail in some context (expected-negative), record it as `pass` with the expectation noted, not `fail`. **Evidence:** verification[] launchd entry marked fail on 4a3b54d, then re-run on e92c663.
3. **Target:** `workflows/coding/skills/coding-task/templates/pr-body.md` (or equivalent). **Change:** put the recording as a link in a titled section, not a bare local path at the bottom. **Evidence:** PR #60 body last line.
4. **Target:** Scribular `CLAUDE.md`. **Change:** note that `uitest doctor` must run inside tmux session `cwf-watch`-owned tmux and that launchd results of exit 1 are expected. **Evidence:** operator_note.

## Human Verification Needed
None. `unverified_manual_count` is 0.
