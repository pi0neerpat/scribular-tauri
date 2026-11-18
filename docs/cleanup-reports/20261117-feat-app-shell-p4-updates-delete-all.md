# cwf cleanup report: app-shell P4 (updates, Delete All Sessions)

Task 20261117-feat-app-shell-p4-updates-delete-all. PR pi0neerpat/scribular-tauri #30, MERGED, merge commit
85046184572b0185323b2a4b170bf58569654f95, final head bba843c. Spec: plans/2026-11-16-app-shell-spec.md (R8, R10, R11; AS8, AS11-AS14, repeat AS5).

## Summary
- Outcome: merged by Patrick after product review at bba843c. Cubic only; final Cubic summary "all reported issues addressed", 23 review threads resolved.
- Patrick confirmed AS5 and AS12 manually. AS8 passed automated (Rust, Vitest, signed-app uitest on an earlier head; the final package still lists a signed Settings walkthrough as pending).
- AS14 (signed update installs via each alert choice) was DEFERRED by Patrick: no signed test update or feed exists. It is not verified. Needs a follow-up when a test feed exists.
- Patrick reported tray "note style buttons" (AS17/AS18). Code on bba843c and main has only Show / Start-Stop with speaker submenu / Quit (src-tauri/src/shell/tray.rs:100-160). Probably an older build was launched. Unresolved: unknown.
- Cost drivers: 6 fix pushes, one 4-hour human wait before the first product review, and a verify loop (about 7 sessions) caused by macOS permissions on the tmux host.
- Token totals are mostly unknown (4 of 22 sessions have transcript data); see Tokens.

## Metrics
| Metric | Value | Source |
| --- | --- | --- |
| Active time | 9357 s (2.6 h) | p4-report.json |
| Human-wait time | 71470 s (19.9 h) | p4-report.json |
| CI-wait time | 4207 s (1.2 h) | p4-report.json |
| Sessions | 22 (build 6, verify 11, review 5 incl. 3 Claude/sonnet) | session_tokens.py |
| Fix pushes | 6 | p4-report.json |
| Resumes | 1 (at 23:27Z, session_recovery at 23:51Z) | events.jsonl:5,8 |
| Compactions / review rounds / premature stops / stop blocks / false_ready | 0 / 0 / 0 / 0 / 0 | p4-report.json |
| Interventions (4 prompt logs) | 1 human-authored (prompt-0004 operator note); 0001-0003 are automated phase-start prompts | logs/prompt-000N.txt |
| needs_human events | 6 (2 product_review_ready, 2 blocked, 2 explicit) | events.jsonl |
| reviewer_network_error | 12 | events.jsonl |
| unverified_manual | 6 (AS5, AS8, AS12, AS14, AS17, AS18 in final package) | p4-report.json, PR comment |
| Demos | 1 final happy-path (bba843c); 5 demo_added, 1 demo_waived (8744f88) | events.jsonl:232 |
| Tokens | known 6.28M over 4/22 sessions (build 5.00M, review 1.28M); the other 18 unknown (no transcript usage) | session_tokens.py |

## Process review

### Value per session
- Session 1 build (30 min, 5.0M tokens): implemented P4, pushed 9e0cd06. High value.
- Session 2 build (12 min, after resume/recovery): no tool calls recorded and no token data. Low visible value; it ran after session_recovery, likely idle recovery. Unknown.
- Verify 1 (10 min): signed app passed 29/29 steps; added the first demo. High value.
- Review 1-3 (Claude sonnet, 2.5, 1.1, 1.7 min, 0.5M, 0.38M, 0.40M tokens): triaged Cubic threads, fixed six at 27deba2. The orchestrator fix in 27deba2 changed the automatic-updates default, so the AS8 evidence became stale. Good catch via operator note.
- Review 4 (codex, 8 min): re-ran AS8, re-recorded the demo, posted package at 27deba2. High value.
- 216.7 min human wait, then build 35 min (a986a25: 11 unresolved Cubic threads follow). Verify 10 min, review 19 min. Fine.
- Verify loop, 5 verify sessions of about 1.5 min each (05:37 to 05:49Z) plus one 14.6 min: zero progress. Pure waste; see Classified items 1.
- Final build 5.7 min (bba843c, four Cubic findings incl. lost-source tray regression), verify 6.8 min, review 3.4 min. Efficient.

### Handoff communication drops
- verify->verify handoff notes (events 183-211) repeated "needs_human" five times with the same blocker; the supervisor only converted this to a block after the 6th (event 214/217). Fix cwf 1b59380 now blocks on this.
- Review-1 hand-off: the stale AS8/demo evidence was not flagged by the session; the operator had to say it (prompt-0004). The review prompt/skill does not require invalidating verification when a fix touches a behaviour covered by AS evidence.
- Demo: demo_waived at 8744f88 ("Patrick will record the demo himself") then a recorded demo on bba843c. Waiver was premature and later superseded.
- Patrick's tray note-style-buttons report arrived with no build identity (SHA or bundle version), so the review could not tell a stale build from a code defect.

### Time per session and idle gaps
Durations are in the Tokens command output. Largest idle gaps: review->build 216.7 min (Patrick product review), verify->verify 10.7 min (tmux restart), review->review 3.9, 4.0, 7.8 min between Claude review sessions. Other gaps under 5 min.

### Tokens
Known: build session 1 5,001,587 (cached 4.81M of 4.98M input); review sessions 496,864, 382,095, 400,874. Unknown for 18 Codex sessions (no transcript usage found). Not estimated.

### Parallel implementation
None used. Single build session at a time; P4 was a single spec slice. No evidence of parallel agents or worktrees.

### PR hygiene
- One package comment (issuecomment 6965488968), edited in place; `package_posted` fired twice with the same comment id (events 94, 306). No duplicate package comments. OK.
- PR body first line is `Recording: <path>`, followed by Summary. OK. The same Recording line is at the top of the package comment. Local /Users path is on the recording line only, as the skill allows.
- Cubic left 4 summary comments (normal). Final package listed AS5, AS8, AS12, AS14, AS17, AS18 under Needs Patrick: 5 of 6 items are manual-only per spec; AS8 listed as pending because the final-head signed walkthrough was not rerun after fixes.
- Package says "Recording ... mp4" under runs/ in the repo working tree (not committed), consistent with CLAUDE.md.

### Demo evidence
One happy-path demo for bba843c (sidebar Delete All, Cancel, confirm, empty list; 16/16 recorded steps, 17/17 unrecorded). It does not show updates or AS8; the demo script for AS8 was only used on 27deba2. Demo kind adequate for the PR but does not cover the whole slice (updates/tray/menu are unverified by recording).

## Classified items
| # | Item | Class | Evidence | Note |
| --- | --- | --- | --- | --- |
| 1 | Verify->verify loop (6 sessions, about 26 min): uitest doctor "MISSING Accessibility/Screen Recording (allow tmux)" after the orchestrator restarted the cwf-watch tmux server itself | model_error (orchestrator) with an infrastructure component | events.jsonl:183-217; orchestrator transcript ~/.claude/projects/-Users-admin-Projects-Scribular-scribular-tauri/b528d4ad-04e6-40ae-878e-8ab2e3249d7c.jsonl | Root cause is the orchestrator restarting tmux from an agent: macOS TCC grants Accessibility and Screen Recording to the launcher, so a tmux started by the agent lost them. Infrastructure component: TCC grants are per launcher and the verify prompt has no loop guard. Patrick restarted tmux from his Terminal. cwf fixes already landed in clauffice (feat/cwf-multi-reviewer): 1b59380, 905f8b5. |
| 2 | Review session could not run `cwf package post` | missing_capability | events.jsonl:301-306; handoff note "package metadata directly to task state, outside the session report" | Writes task state outside the Codex/Claude sandbox. Blocked needs_human; orchestrator posted package and ran `cwf resume --to technical_review`. Cost about 1 min plus a human step. |
| 3 | Cubic reviewer_network_error x12 | infrastructure | events.jsonl (12 events) | Polling retries; no task impact beyond latency. |
| 4 | Stale AS8 evidence after fix 27deba2 changed default | model_error (review session) | prompt-0004 operator note; events.jsonl:74 | Reviewer did not invalidate verification; operator caught it. |
| 5 | Premature demo waiver at 8744f88 | model_error (verify session / process) | events.jsonl:232 vs :280 | Waiver later superseded by a real demo; no harm. |
| 6 | Tray "note style buttons" report | unknown | PR comment AS17/AS18; tray.rs:100-160 | Likely older build; no build identity captured. |
| 7 | AS14 deferred | missing_capability | PR package; Patrick note | No signed test update or feed exists in the environment. |
| 8 | Session 2 build (12 min, no tool calls, no tokens) | unknown | session_tokens.py output; events.jsonl:8 | Following session_recovery. |
| 9 | 18 of 22 sessions without token data | infrastructure | session_tokens.py | Codex transcripts lack usage. |

## Proposed skill changes (proposals only, not applied)
Target: ~/Projects/clauffice/workflows/coding/skills/coding-task/SKILL.md unless stated.

1. Package post from sandboxed review sessions (SKILL.md around line 216).
```diff
-   Post with `cwf package post --file <md> [--pr N]` only; it edits the existing package comment.
+   Post with `cwf package post --file <md> [--pr N]` only; it edits the existing package comment.
+   If the sandbox blocks it, write the package to the session report as `package_file: <path>` and
+   hand off; the supervisor runs `cwf package post` from outside the sandbox. Do not use needs_human.
```
(Implementation: bin/cwf handoff reads `package_file`, supervisor posts it. Alternative: allowlist the task state dir in the review sandbox.)

2. Operators must not restart the supervisor tmux server from an agent. Add to the operator/supervisor section of SKILL.md and README.md:
```diff
+ Never restart or recreate the cwf-watch tmux server, or any session hosting uitest, from an agent
+ session. macOS TCC grants Accessibility and Screen Recording to the launching terminal; an
+ agent-launched tmux loses them and verify loops on `uitest doctor` "MISSING". Ask Patrick to
+ restart it from Terminal.
```

3. Verify prompt: stop on first doctor failure.
```diff
+ If `uitest doctor` reports MISSING Accessibility or Screen Recording, hand off `needs_human`
+ once with the exact command and stop; do not retry or hand off verify->verify.
```
(1b59380 and 905f8b5 already block same-phase loops; keep this as the in-prompt rule.)

4. Review phase: invalidate evidence after a behaviour fix.
```diff
+ After a review fix, list each AS whose behaviour the diff touches. Mark its verification and any
+ demo stale (`cwf demo` shows stale) and re-run them, or hand off to verify. Do not carry evidence
+ from an earlier head over a behaviour change.
```

5. Product package/PR template (templates/review-package.md): add `Build: <head sha>` and a "Reporting feedback" line asking Patrick to name the build SHA or app version he tested, so reports like the tray buttons can be matched to a build.

6. cleanup/SKILL.md: have `session_tokens.py` print "unknown for N sessions (no transcript usage)" in the summary and have Codex adapters keep usage records.

## Not verified here
Patrick's tray report, AS14, and token totals for 18 sessions. Per-session durations come from session_start/session_end via session_tokens.py.
