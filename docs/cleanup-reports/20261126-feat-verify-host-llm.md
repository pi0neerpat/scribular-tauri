# Cleanup report: 20261126-feat-verify-host-llm

## Summary
Merged. PR #65 (`feat/verify-host-llm`) merged as `c601407cc61d4580f08cf9be5023559f0359e779`; head at review was `eb3e6da`. uitest now uses the host llama-server automatically in the Tart VM. The run was smooth: one Cubic round (8 valid threads, all fixed in one build session), no premature stops, no false-ready, no compactions, no real interventions. The one time sink was the first verify session (47 min), whose cause is not recorded (see Time). This is an automatic-mode run: step 8 skipped, all fixes below are proposals.

## Metrics
| Metric | Value |
|---|---|
| Terminal state | cleanup (PR merged) |
| Active / human-wait / CI-wait | 3635 s / 19882 s / 632 s |
| Review rounds / fix pushes | 1 / 2 (cwf counter `counters.fix_pushes` shows 0; report shows 2) |
| Compactions / resumes | 0 / 0 |
| Premature stops / stop blocks / false-ready | 0 / 0 / 0 |
| Interventions | 1, filtered (see below) |
| Unverified manual / demos | 0 / 0 (demo waived: uitest-only change) |
| Phase settings | `~/.local/state/clauffice/workflows/phase-settings.json`; `phase_settings_drift`: none |
| Self-task | no (no `deployed_commit`) |

## Sessions and models
| Phase | Harness / model | Wall | Tokens |
|---|---|---|---|
| build | codex / unknown | 1.35 min | unknown (no transcript usage) |
| build | claude / opus | 2.12 min | 1,177,160 |
| verify | claude / sonnet | 47.31 min | 2,981,324 |
| review | claude / sonnet | 1.23 min | 240,064 |
| build (fix) | claude / opus | 2.27 min | 762,581 |
| verify | claude / sonnet | 5.96 min | 555,276 |
| review | claude / sonnet | 1.43 min | 736,079 |
| cleanup | claude / sonnet | 0.24 min at count time | 167,396 |

Totals (known sessions, 7/8): 6,619,880 tokens. build 1.94 M, verify 3.54 M, review 0.98 M, other 0.17 M. Verify is 53% of tokens.

## Classified items
| Item | Class | Evidence |
|---|---|---|
| Intervention 1 at 2026-11-27T03:37:25Z | not an intervention: it is the cleanup prompt (`logs/prompt-0001.txt`); filtered | no human prompt during the run |
| First build session (codex) ended with no handoff | `infrastructure` | events: `session_usage_limit` "try again at Nov 29th, 2026 8:17 PM", `needs_human` "codex usage limit hit"; cwf fell back to claude/opus within 1 s of `session_end` |

## Process review
- **Value per session.** Build (opus) 2 min: produced HostLLM and wiring, useful. First verify 47 min: useful (ran swift test, built, ran the VM scenario) but long. Review sessions (1.2 and 1.4 min) each read Cubic state and wrote a handoff; the first correctly turned 8 threads into one fix list, the second only confirmed approval and posted the package. Fix build 2.3 min plus re-verify 6 min: re-ran `swift test` twice and the VM smoke scenario; justified because HostLLM.swift changed. No same-head double review.
- **Communication.** The verify handoff (build note, event line 13) listed the doctor line, overlay feature and the log check; verify exercised all. The review handoff listed all 8 Cubic findings with file names; the fix build resolved them (Cubic final summary: "All reported issues were addressed"). No dropped items found. `handoffs/*.md` does not exist in the task dir (only `reports/*.ingested.json`), so handoff text was read from event `note` fields.
- **Time.** build(codex) 20:55:31-20:56:52, build 20:56:52-20:58:59, verify 20:59:02-21:46:21, idle 3.4 min (Cubic running, one `reviewer_network_error` at 21:48:45), review 21:49:43-21:50:57, fix build 21:50:58-21:53:14, verify 21:53:32-21:59:30, idle 4.2 min, review 22:03:40-22:05:06, then 332 min wait for Patrick's merge (human-wait). Environment gates that stalled verify: none recorded; the 47 min is unexplained (the `pr_linked` event and `swift test` land in its last 15 s). Unknown: what the session spent time on, since the transcript was not read in detail.
- **Tokens.** From `session_tokens.py`, table above. Codex build tokens unknown.
- **Parallel implementation.** The plan did not recommend splitting; one build agent did it.
- **PR hygiene.** PR #65 has one Cubic review summary and one product-review package comment, no duplicates, package head `eb3e6da` matches the head (`package_stale: false`). No recording (waived).
- **Demo evidence.** Waived: uitest-only change (`demo_waived`). Nothing to flag.
- **Product review package.** Titled "Recording" not "Proof of success", has no mermaid journey (acceptable only because the demo was waived), and the "Test it yourself" section reads "None" instead of "Human Verification Needed". See proposal 1.

## Proposed changes (not applied; automatic mode)
1. **Target** `workflows/coding/skills/coding-task/templates/review-package.md` (or the code that renders it). **Change:** for a waived demo, render the section as "Proof of success" with the waiver reason, and name the human section "Human Verification Needed" (with "None" when empty). **Evidence:** PR #65 package comment uses "Recording" and "Test it yourself". Confidence low: I did not read the template, so this may already be handled and the posted package may be from an older template.
2. **Target** cwf phase prompt for verify (`workflows/coding/skills/coding-task`) and `watcher-config.json` `operator_note`. **Change:** ask the verify handoff to state wall-time-heavy steps, so a 47 min verify is explainable. **Evidence:** events 17-23 show no step timings. Confidence low.
3. **Target** cwf `cwf status` counters. **Change:** reconcile `counters.fix_pushes` (0) with `cwf report` `fix_pushes` (2). **Evidence:** both outputs on this task. Possibly different definitions; check.
4. **Target** this project's docs (`tools/uitest/README.md`). **Change:** none needed; PR #65 already documents the VM behaviour. The `host-llm-smoke.uitest` scenario lives only in the task state dir (`scenarios/`) and was not committed; consider adding it to `tools/uitest/scenarios/` in a later task. **Evidence:** `docs/remote-llm-uitest.md` known limits.

## Docs and PR
Docs: `docs/remote-llm-uitest.md` updated for the automatic host server (branch `docs/cleanup-20261126-feat-verify-host-llm`). Report copy: `docs/cleanup-reports/20261126-feat-verify-host-llm.md`.

## Not verified here
No acceptance scenarios were run in cleanup. Ignored evidence in the clone: `runs/` (uitest run folders, not cited by the PR). No manual-only scenarios exist.
