# Cleanup report: 20261121-feat-uitest-queue-keychain

## Summary
Merged. PR #44 (pi0neerpat/scribular-tauri) merged 2026-11-21T03:08:13Z at d3886d24847a52389d4d4807ed6cffdfc0e72357. Queueing on the uitest desktop lock plus keychain auto-answer. Clean run: zero interventions recorded by cwf, no premature stops, no compactions. Three operator observations (below) are workflow-config friction, not code failures.

## Metrics
| Metric | Value |
|---|---|
| Active / human-wait / CI-wait | 33.6 min / 1.1 min / 11.2 min |
| Review rounds (cwf) / fix pushes | 0 / 2 |
| Compactions / resumes / premature stops / stop blocks / false ready | 0 / 0 / 0 / 0 / 0 |
| Sessions | 6 codex (build, verify, review, build, verify, review); model identifier per events: None; transcripts name gpt-6-sol |
| Tokens (estimated) | 16.47M total (build 4.18M, verify 10.05M, review 2.24M); uncached 0.63M |
| Demo | 1 happy-path mp4, not stale |

## Classified items (cwf recorded 0 interventions; these come from the operator)
| # | Item | Class | Evidence |
|---|---|---|---|
| 1 | `cwf init` without `--start` leaves task in spec_agreed; no CLI starts it. Operator called state.transition + supervisor.tick | missing_capability | events.jsonl line 2: "started by operator (cwf init --start equivalent)" |
| 2 | Reviewer configured `agent` from ~/.claude/CLAUDE.md and spec Execution notes; coding-task skill says Scribular stays on cubic; Patrick corrected; `cwf set-reviewer cubic` | bad_instruction | events line 6 reviewer_changed agent -> cubic (about 1 min after start); spec "Reviewer: `agent`" line |
| 3 | Patrick asked for codex default profile; CLAUDE.md says codex-bravo via `set-codex-home` | product_uncertainty (preference, not a failure) | no set-codex-home was set; sessions ran harness codex |

## Process review
- Value per session: build 1 (5.6 min) and verify 1 (13.3 min) produced the PR and native AS1-AS8 evidence; review 1 (2.4 min) triaged 9 Cubic findings (resolved the AXDriver Deny thread as spec-required, 8 sent to build). Build 2 (3.5 min) fixed them. Verify 2 (9.9 min) re-ran the whole native matrix (AS4-AS7, recording, screenshot) although the delta touched only lock/keychain/log/test code; partial overlap with verify 1, but AS6 trust-log append and lock changes justified the queue and keychain reruns, the recording rerun is arguable. Review 2 (2.9 min) was a no-code session: Cubic approved the head and it only replied to one outdated thread and posted the package; this could have been done by the supervisor without an agent session.
- Communication: handoffs were followed. Review->build (e5528e4) listed eight threads; build handoff (26a53f9) covers all eight by name. Dropped: none found. The one Cubic thread left on head 7b7b722 (QueueKeychainTests python3) was already fixed in the build, stale-thread noise handled in review 2.
- Time: build 5.6, verify 13.25, review 2.4, build 3.5, verify 9.9, review 2.9 min. Idle gaps: 4.2 min verify->review (Cubic wait), 1.6 min verify->review. No stalls. Smoothest: build 2 -> verify 2.
- Tokens: see table; verify dominates (61%) from large cached context. Per-session usage known for 6/6.
- Parallel implementation: spec said one PR, S-M; done by a single agent each phase; no split recommended.
- PR hygiene: one Cubic summary comment and one product package comment (7056381325, posted once); no duplicates. PR body has Cubic auto summary plus the Recording path line at the top is a full local path (recording is on the VM, not linked), no filename heading. Package "Test it yourself" says "None."; Evidence files for runs/ exist locally.
- Demo evidence: happy-path only, named by spec; paced via recorded run. Surface (native window) captured.

## Proposed changes (not applied; cleanup step 8 skipped by operator instruction)
See the report returned to the supervising session for diffs: (a) coding-task skill: start step after `cwf init`; (b) reviewer selection precedence; (c) specs from write-spec should not hard-code reviewer. Observation 2 CLAUDE.md wording proposal: replace "`--reviewer agent --agent-reviewer-login pi0neerpat --phase-harness review=codex`" in the cwf bullet with "`--reviewer <per-repo: cubic for Scribular, agent otherwise; see coding-task skill>`", and the "Technical review: the local codex `agent` reviewer, not GitHub Cubic" sentence with "except repos the coding-task skill pins to cubic (Scribular)".
