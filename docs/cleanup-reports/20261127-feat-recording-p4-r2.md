# Cleanup report: 20261127-feat-recording-p4-r2

## Summary

- **Outcome: merged.** PR #68 (`feat/recording-p4`, pi0neerpat/scribular-tauri) merged at `aebc9a997694b8d22577c08daf606b4be2ae48c7` on 2026-11-27 22:38:58Z. Final head `9404fe2`.
- Scope: AS18 fault-injection privacy tests, AS17 walkthrough checklist, 5 Cubic fixes, then 3 defects Patrick found in the manual AS17 walk (pending audio shown as "not kept" during finalizing, near-silent meters, flashing app-audio permission notice).
- Wall clock was dominated by waiting: 14.4 h waiting on a human, 0.9 h active agent time, 1.0 h CI/reviewer wait.
- Two infrastructure stalls cost the most: codex-bravo usage limit (review session produced nothing, about 9 h gap) and a stale `tmux_name` that sent the first build into `blocked` after three 1-minute recoveries.
- AS17 stayed unverified by design (manual-only); Patrick walked it and fed back. The posted review package covers head `4bec110` only; the merged head `9404fe2` has no package on the PR (see PR hygiene).
- Step 8 skipped (automatic mode). Proposed fixes are listed at the end; none are applied.

## Metrics

| Metric | Value |
|---|---|
| Terminal state at cleanup start | `cleanup` (merged) |
| Active / human wait / CI wait | 3344 s / 51869 s / 3512 s |
| Review rounds / fix pushes | 0 / 3 (report counters) |
| Compactions / resumes | 0 / 0 |
| Premature stops / stop blocks / false ready | 0 / 0 / 0 |
| Interventions (raw) | 3; after filtering the cleanup task prompt: 2 |
| Unverified manual | 1 (AS17) |
| Demos | 0 (waived: no UI change at the time of waiver) |
| Phase settings | `~/.local/state/clauffice/workflows/phase-settings.json`; `phase_settings_drift`: none |
| Harness config | claude, review phase overridden to claude, reviewer cubic |

## Sessions (models, wall time, tokens)

From `session_tokens.py`. Codex usage unknown (no transcript usage).

| Phase | Harness / model | Wall | Total tokens |
|---|---|---|---|
| build (x3 recoveries) | claude opus | 0.9 + 1.1 + 1.3 min | 276k + 183k + 344k |
| build | claude opus | 2.9 min | 1.03M |
| verify | claude sonnet | 1.7 min | 392k |
| review | codex gpt-6.2-sol | 25.1 min | unknown (0 recorded) |
| review | claude sonnet | 0.6 min | 237k |
| build (Cubic fixes) | claude opus | 2.9 min | 1.27M |
| verify | claude sonnet | 1.2 min | 291k |
| review | claude sonnet | 1.3 min | 640k |
| build (product feedback) | claude opus | 5.3 min | 3.30M |
| verify | claude sonnet | 36.4 min | 1.54M |
| cleanup | claude sonnet | 0.25 min (prompt only; this session continued) | 128k |

Totals: build 6.40M, verify 2.23M, review 0.88M (known sessions), other 0.13M; grand 9.63M over 13 sessions. Idle gaps: review to review 543.9 min (usage limit), verify to cleanup 294.7 min (waiting for Patrick's merge), review to build 45.1 min (human), build to build 3.3 min.

## Process review

**Value per agent session.**
- Build recoveries (06:21 to 06:25): three opus sessions of about 1 minute each did nothing (`session_recovery`, then `blocked: recovery limit reached` at 06:25:11). Cost 0.8M tokens, zero commits. Cause was a stale `tmux_name` from a cancelled task that prefix-matched this session; the supervisor note at 06:28:15 says it cleared it.
- Review on codex (06:40 to 07:05, 25 min): died on the codex usage limit (`session_usage_limit` 06:41:25, "auto-resume after 20:17"). No review was produced. The task sat in `technical_review` until 16:07, when it was restarted on claude.
- Verify at 06:31 and review rounds at 16:09 and 16:17 were efficient (1 to 1.3 min). No same-head double review.
- The 36-minute verify at 17:08 re-ran cargo and vitest that the build session (opus, 5 min) could not have finished, which is legitimate. It also re-ran `system-audio-smoke-57s` and proved on origin/main that it fails identically (Whisper "neighborhood" variance): useful, but the failing entry sits in the verification list on this head and is a pre-existing flake.

**Communication (handoffs vs next session).**
- Build to verify (6772f45): AS17 listed as manual-only unverified. Carried through every handoff. OK.
- Review (16:09) listed 5 Cubic threads; build fixed all 5 in 4bec110. Nothing dropped.
- Verify at 17:45 handoff says "First action next: review current head". The next step (technical review) approved in 34 s (17:46:16) with "package posted on this head", but the PR has one product-review comment, for head `4bec110`. Either the package was edited or it was not re-posted; the PR shows only the `4bec110` text. The handoff ask was met only by the reviewer's approval, not a fresh package.
- Prompt 2 ("a cwf command was blocked. Can you add any cwf action as an approved command?") asked for a permission change; no handoff references it, so the follow-up is unknown.

**Time.** Active agent time was 56 min across 13 sessions. Idle: 9 h waiting on codex quota, 4.9 h between verify and merge. Human gate: product_review_ready at 16:18, feedback at 17:03 (45 min), re-ready at 17:46, merge at 22:39.

**Environment gates that stalled verify.** None stalled verify this run (preflight exit file present in `logs/preflight.exit`). The codex usage limit stalled review (infrastructure). Cubic network errors repeated about every 40 s for hours (`reviewer_network_error` 06:33 to 07:48+).

**Tokens.** See table. Codex totals unknown.

**Parallel implementation.** The spec (`plans/2026-11-26-recording-p4-spec.md`) did not recommend splitting across agents. Single agent builds. Not applicable.

**PR hygiene.** `gh pr view 68 --comments`: one cubic summary (head 6772f45) and one product-review package (head 4bec110). No duplicate comments. The package is stale relative to the merged head 9404fe2, and its "Recording" section says "None: no UI change", while the final head changed UI behavior (Audio tab availability, meters, notice). The package has no "Proof of success" heading or mermaid diagram, and the section is titled "Test it yourself", not "Human Verification Needed" (no demo existed, so "Proof of success" is moot, but the title rule still applies). Its AS17 item points to a table instead of numbered steps. Evidence file `plans/evidence/recording-p4-as17.md` exists.

**Demo evidence.** None recorded. Waiver reason ("No UI change") was true at 4bec110 but stopped being true after product-feedback fixes at 9404fe2; the waiver was not revisited. The meter and Audio-tab changes could have been captured by uitest (AS15 playhead scenario passed on this head).

**Product review package.** See PR hygiene: wrong section title, no numbered one-action-per-step list, no "Proof of success" block.

## Classified items

| # | Item | Class | Evidence |
|---|---|---|---|
| 1 | Three 1-minute build recoveries then `blocked` | infrastructure | events 06:22:46 to 06:25:11; supervisor note 06:28:15 "cleared cancelled task's stale tmux_name that prefix-matched this session" |
| 2 | Intervention 1 "you run it" (06:28:11) | infrastructure | follows the `blocked` above; Patrick restarted the build |
| 3 | Intervention 2 "a cwf command was blocked... add any cwf action as an approved command... fix the cwf bug" (06:31:01) | missing_capability | `logs/prompt-0002.txt`; a cwf action lacked an allow rule in the agent's permissions |
| 4 | Intervention 3 (22:40:10) | filtered | this cleanup task prompt (`logs/prompt-0003.txt`), not a human correction |
| 5 | Codex review died on usage limit; 9 h idle | infrastructure | `session_usage_limit` 06:41:25; transition 16:07:51 "codex-bravo out of quota until Nov 29" |
| 6 | Cubic network errors for hours | infrastructure | `reviewer_network_error` events from 06:33 |
| 7 | Three defects found only in Patrick's manual AS17 walk (R15, meter, notice flash) | product_uncertainty | `product-feedback-1.md`; AS17 manual-only by spec |
| 8 | Demo waiver not revisited after UI-affecting feedback | bad_instruction | `demo_waived` 16:18:20 on 4bec110; no event after 9404fe2 |
| 9 | Review package not re-posted for 9404fe2 | bad_instruction | PR comments show one package, head 4bec110; transition 17:46:16 |
| 10 | `system-audio-smoke-57s` fails on branch and main | infrastructure | verification entry, fails at step 30 on origin/main too |
| 11 | Premature stops, false ready, compactions | none | report counters all 0 |

## Proposed changes (not applied; automatic mode)

1. **Re-evaluate the demo waiver after feedback builds.**
   - Target: `workflows/coding/skills/coding-task/SKILL.md` (feedback/verify section).
   - Change: after a product-feedback build, clear `demo_waived` and require the verify or review session to decide again against the new diff.
   - Evidence: waiver set on 4bec110 (events 16:18:20) stayed in force at merge though 9404fe2 changed visible UI (item 8).
2. **Re-post the product review package on every new head before approval.**
   - Target: `workflows/coding/skills/review/SKILL.md` (and `templates/review-package.md`).
   - Change: refuse `product_review_ready` unless a package comment names the current head sha.
   - Evidence: transition at 17:46:16 "package posted on this head" while the only package comment says head 4bec110 (item 9).
3. **Package template wording.**
   - Target: `workflows/coding/skills/coding-task/templates/review-package.md`.
   - Change: title the human section "Human Verification Needed" with numbered one-action steps; add "Proof of success" with a mermaid journey when a recording exists, else a one-line reason.
   - Evidence: the posted package uses "Test it yourself" and a table reference.
4. **Backstop stale `tmux_name` prefix matching.**
   - Target: supervisor source under `workflows/coding/` (session lookup by tmux name).
   - Change: match the full `=name` session, not a prefix, and drop `tmux_name` when a task is cancelled.
   - Evidence: events 06:22 to 06:25 and the 06:28:15 transition reason (item 1).
5. **Allow cwf actions for phase sessions.**
   - Target: harness permission template used for phase sessions (project allowlist).
   - Change: allow `cwf *` subcommands the skills call so none prompts or blocks.
   - Evidence: prompt 2 (item 3). Exact blocked command is unknown; no log names it.
6. **Fall back automatically when codex hits its quota.**
   - Target: `watcher-config.json` handling in the supervisor (`session_usage_limit`).
   - Change: when the limit's reset is more than an hour away, switch the phase harness to claude without a human step.
   - Evidence: 9 h idle between 07:05 and 16:07 (item 5); a human restarted the review on claude.
7. **Project-doc note for the flaky smoke scenario.**
   - Target: `tools/uitest/README.md` (product repo, outside docs/ so a later PR).
   - Change: note that `system-audio-smoke-57s` step 30 ("neighborhood") varies with Whisper output and to compare against origin/main before treating it as a regression.
   - Evidence: verification entry, same failure on main (item 10).

## Notes

- Spec part: `spec.part` is not set; no next-part command.
- Docs and report PR: see `docs/cleanup-reports/20261127-feat-recording-p4-r2.md` in the product repo (PR link recorded in the final session message).
