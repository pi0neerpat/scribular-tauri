# Cleanup report: 20261119-feat-note-style-coach (PR #38)

## Summary

- **Outcome:** merged. PR #38 `Add native Note Style Coach`, merge commit `7a4f279540d8d0aad9e23713d67c8104c45442b3` (2026-11-19T22:11:41Z). Final head `37e6dc8`.
- **Shipped:** three-step Note Style Coach (sample or own example note, questions, practice with Before/After comparison, save), recording-priority gating, Record-screen Regenerate preview fix, plus the merge of origin/main (#34-#36, remote-llm host llama-server).
- **Not proven:** AS1 tray interruption (uitest cannot press tray items; Patrick verified manually) and AS15 (manual-only). AS5/AS6/AS12 rest on fake-engine tests.
- **Process:** 18 sessions (build 8, verify 6, review 4), ~5.8 h real agent/work time plus a 14 h unexplained stall, 75.5M tokens known (6 codex sessions have no usage data). 3 review passes (Cubic 20 threads, Cubic cap, Opus agent 4 blocking), 3 fix pushes, 3 demos (2 stale issue clips, 1 current happy path).
- **Main avoidable costs:** (1) recovery limit blocked the task because `head_sha` never followed local commits before a PR existed (stale `258ec04` through 4 local commits); (2) real-model stalls: two 600 s uitest waits (events 29, 52) and a 95 min verify that ended without a handoff; (3) the operator note told the agents to "confirm" a log line and Codex changed product license diagnostics to emit it; (4) supervisor-side errors (llama-server started inside the VM, loopback URL in the operator note) cost one invalidated run.
- **Main human cost:** Cubic cap (needs_human, ~6 min to reviewer switch), Codex usage limit (manual move to codex-bravo), 3 manual unblocks of the supervisor (events 68-72, 159-162).

## Metrics

| Metric | Value |
| --- | --- |
| Terminal state | cleanup (PR merged) |
| Active / human-wait / CI-wait (cwf) | 71719 s (19.9 h) / 1279 s (21 min) / 1422 s (24 min) |
| Active, excluding the 05:29-19:32 stall | ~21000 s (~5.8 h): the stall (845 min, `session_tokens.py`) is booked as `active` because the task stayed in `verifying` |
| Sessions | 18: 8 build, 6 verify, 4 review (5 claude, 13 codex) |
| Review rounds (cwf) / fix pushes | 0 / 3 (the real count: Cubic round 1 = 20 threads, Cubic cap, Opus round 1 = 4 blocking, Opus round 2 = approve) |
| Compactions / resumes (cwf) | 0 / 2. Events 6-7 are two `resume source=compact` in session 1; no `compaction` event is ever written, so cwf under-reports compactions (see proposal 2) |
| Premature stops / stop blocks / superseded | 0 / 0 / 0 |
| Sessions ended without handoff | 3 (events 8, 33, 67) |
| False-ready | 0 |
| Unverified manual items | 2 (AS1 harness gap, AS15 manual-only) |
| Demos / stale demos | 3 / 2 (AS3 issue clip at b895b21, AS8 issue clip at eb07b7d, happy-path at 37e6dc8 current) |
| Screenshots | 9 events |
| Reviewer network errors | 0 |
| Sessions hitting usage limit | 1 (codex, event 118) |

Models: session_start `model` is `opus` for the first build session, `sonnet` for review sessions 9 and 11, `opus` for review sessions 12 and 13, and `null` for all codex sessions. The token script reads `gpt-6.2-sol` from codex transcripts; exact Claude IDs are not in the events (unknown).

## Process review

**Value per agent session**

| # | Phase (harness/model) | Wall | Tokens | Verdict |
| --- | --- | --- | --- | --- |
| 1 | build (claude opus) | 12.6 min | 4.49M | Two compactions, ended without a handoff (event 8, cause unknown); work was continued by codex from the same worktree (harness change by the operator; reason not in events). |
| 2 | build (codex) | 41.6 min | 16.3M | Valuable: whole feature, handoff 17 with counts. |
| 3 | verify (codex) | 17.6 min | 8.4M | Valuable: found the duplicate-AX Skip selector and a 600 s practice stall (event 29), issue demo AS3. |
| 4 | build (codex) | 13.0 min | 0.87M | Ended without handoff (event 33); no usage data after this point for 6 codex sessions. |
| 5 | build (codex, recovery) | 5.3 min | unknown | Produced the handoff (event 39) that session 4 should have. Recovery prompt-0002 says "previous phase exited without a handoff". |
| 6 | verify (codex) | 39.2 min | unknown | Valuable but slow: Make-it-shorter produced no reply and comparison timed out at 600 s twice (event 52), issue demo AS8. |
| 7 | build (codex) | 13.3 min | unknown | Fixed empty-optional-heading scoring, diagnosed on real Gemma (40/92/47 s). |
| 8 | verify (codex) | 95.1 min | unknown | Poor: re-ran the same stalled native AS8 against on-device Gemma, ended without a handoff (event 67). Then nothing for 14 h. |
| 9 | build (codex) | 34.5 min | unknown | Merge of origin/main + scenario fixes + host-LLM runs. ~16 min of it was the invalid 127.0.0.1 run (supervisor correction, prompt-0007). |
| 10 | verify (codex) | 4.7 min | unknown | Cheap and useful: pushed, opened PR #38, 30/30 on host, happy demo. |
| 11 | review (claude sonnet) | 1.05 min | 0.26M | Router: read 20 Cubic threads (event 112). Useful. |
| 12 | build (codex) | 27.5 min | 18.1M | Fixed 20 findings; also added debug license forwarding to uitest; hit usage limit at 20:33 (event 118), moved to codex-bravo by the operator. |
| 13 | verify (codex) | 12.0 min | 6.4M | Re-ran full scenario, 3 demos. Duplicates build's own native run (event 120) at the same head. |
| 14 | review (claude sonnet) | 1.7 min | 0.50M | Replied to and resolved 4 threads, then blocked on the Cubic cap (event 154). Necessary. |
| 15 | review (claude opus) | 1.07 min | 0.35M | Router for the operator-dispatched agent review (4 blocking). |
| 16 | build (codex) | 7.9 min | 3.75M | Fixed the 4 blockers with regression tests. |
| 17 | verify (codex) | 26.8 min | 15.5M | Re-ran everything, several uitest failures at 37e6dc8 (events 201-206) before a pass on an isolated rerun; 4 timing-sensitive Rust failures from running vitest concurrently. Valuable only partly; see time. |
| 18 | review (claude opus) | 1.7 min | 0.63M | Gate + package. Necessary. |

Overlap: build session 12 and verify session 13 each ran the same native scenario on head `3dee687` (30/30 both times, events 120 and 137). Verify sessions 6 and 8 re-ran the same stalled AS8 scenario on a model path already known to take 600 s. The first Opus review (4 "blocking", 7022334269) and the second (APPROVE on `37e6dc8`, 7023387561) were each followed by a full build then verify cycle (27+12 min, 8+27 min, ~44M tokens) for 4 localized fixes with unit regressions; the verify at the end re-ran full native suites although only quote parsing, heading matching, the Record-screen preview and a retry alert changed.

**Communication (handoff vs next session)**

- Handoff 17 (build to verify) delivered a clean commit; verify found real defects. Smooth.
- Handoff 83 said "VM cannot access the host llama-host.log to confirm /completion lines; verify should check host log". Verify (event 99) records only the app log line. No later handoff or package item confirms `/completion` requests on the host. Dropped (the 30/30 runs and Opus-visible streaming are indirect proof).
- Operator note asked for `/completion` lines in `~/Library/Logs/llama-host.log` (prompt-0006). That file is on the host, unreachable from the VM; the note was written for a host-side agent and repeated unchanged in all later prompts until the correction. The correction (prompt-0007) removed it.
- Handoff 145 said review should resolve addressed threads; session 14 did (4 evidence replies, event 154). Followed.
- Handoff 169's four blockers (quote ranges, rename boundaries, Record-screen preview, retry alert) all appear fixed in `37e6dc8` and re-verified by Opus round 2 (comment 7023387561). Followed. Opus round 2 left 3 non-blocking nits (license diag code, plural possessive quote, preview effect rerun); no one owned them after merge. They are listed in the package only.
- Handoff 183 listed AS1/AS15 as unverified; the package carries them with steps. Followed.
- The operator note's 'SCRIBULAR_SKIP_LICENSE set: access checks skipped' sentence was used as a test oracle; see Classified items.
- Review sessions 14 and 18 received the long merge-origin/main instructions in their prompts after the merge was already done (prompt-0011, 0013). Noise, harmless.

**Time** (from `session_start`/`session_end` and `session_tokens.py`)

- Longest sessions: verify 95.1 min (session 8), codex build 41.6 min, verify 39.2 min, build 34.5 min.
- Stall: 05:29:39 to 19:32:24 (845 min) with no event, state `verifying` and `ended_without_handoff`, no `session_recovery`. Cause unknown (supervisor not running, machine asleep, or state; events.jsonl has nothing between). Ended when the operator transitioned by hand (event 68).
- Gap after usage limit: 20:33 limit, build finished 20:47 after the operator moved the session. Idle gaps in the script: 4.9 min, 0.4 min, 10.1 min (review to review: operator Cubic-skip decision, ~6 min of that is waiting for the human), 12.1 min (verify to review: 11 min of `stale_head` polling, events 214-225, waiting for the operator to post the second agent review).
- Interventions: 13 prompts: 11 standard phase dispatches, 1 operator note carried in prompts (0006 onward), 1 supervisor correction (0007). Post-merge: product_review_ready to merge 9.5 min.
- Smooth handoffs: build to verify (0.01 min gaps) and review to build. Rough: verify 8 to nothing (stall); build 4 without handoff; review wait on agent-review.

**Tokens** (`session_tokens.py`): grand total known 75,514,082 (12 of 18 sessions). Build 43.5M (5 of 8 known), verify 30.3M (3 of 6 known), review 1.74M (4 of 4). Input is almost entirely cache reads (73.5M of 75.5M); uncached input 1.3M. Six codex sessions (02:56, 03:02, 03:41, 03:54, 19:35, 20:09) have no usage in the rollouts found, so the total is a lower bound. Codex counts are estimates from cumulative deltas. Compactions: script says 0; session 1 had two compact resumes (events 6-7).

**Parallel implementation:** the spec suggests a 3-PR split (PR1 engine, PR2 jobs/commands, PR3 UI) and the "L, 12-18 days" size; this task shipped as one PR (#38) by one build agent (session 2 for the first pass, one commit `b895b21`). No subagent or split-session evidence in events or tool counts (tool counts: 14 for session 1, 6 for session 2; the script counts only some tool types). The spec's split was not followed; the diff is 70 files per Cubic's summary. Whether that contributed to the 20 Cubic threads is unknown.

**PR hygiene**

- Comments on PR #38: Cubic's review summary, three `pi0neerpat` comments (Opus review 3dee687, Opus re-review 37e6dc8, product package 7023420065). No duplicate package or repeated headings. Five `COMMENTED` reviews by `pi0neerpat` with empty bodies (inline replies).
- The description starts with `Recording: /Users/.../logs/verify-ozAh42/review/2026-11-16-note-style-coach-spec-happy-path.mp4`, a local absolute path that a GitHub reader cannot open (same line in the package). It names the spec-named happy-path file, which is correct.
- Evidence files that exist: demo mp4 in the task `logs/verify-ozAh42/review/` and 2 issue clips (one in `/private/tmp`, may disappear); `runs/verify-ozAh42/acceptance.md` is in ignored `runs/` of the clone (copy out before deleting). `verify-aDbjQf` has acceptance logs and a happy-path mp4 for the earlier head.
- Cubic's thread count dropped 20 to 4 to 0 on resolution, then Cubic could not run on `3dee687` (monthly cap). The package notes "Cubic skipped" implicitly via the Opus comment headings; it is not in the PR description.

**Demo evidence:** 3 recordings registered. Issue clips for AS3 and AS8 correspond to real unresolved failures at the time (duplicate Skip match, 600 s stall), matching the skill. The happy-path (113 s, paced, spec-named) was recorded on the final head. Two issue demos are stale (older heads) and remain in `task.json`. The tray-interruption surface (AS1) could not be captured: Accessibility press reached the menu-bar item but did not trigger native Start Recording; Patrick verified it manually.

## Classified items

Task-notification prompts filtered: none found (`grep -il task-notification logs/prompt-*.txt` is empty).

| Item | Evidence | Class | Note |
| --- | --- | --- | --- |
| Phase dispatch prompts 0001, 0003-0005, 0008-0013 | `logs/prompt-*.txt` | not interventions | Standard supervisor prompts. |
| prompt-0002 recovery after "exited without a handoff" | events 33, 35; prompt-0002 | model_error | Codex build session ended at 13 min without running `cwf handoff`; recovery session produced it. Cause unknown (usage not recorded). |
| Session 1 ended without handoff after two compactions | events 6-8 | model_error | Unknown cause; harness switched to codex (operator, reason not logged). Low confidence. |
| Session 8 verify 95 min, ended without handoff, then 14 h stall | events 64-68; `needs_human` absent | model_error + missing_capability | Verify kept rerunning on-device AS8 (known 600 s stall, events 29, 52). No `session_recovery` or notification fired for 14 h (cause unknown). |
| Two 600 s uitest waits | events 29, 52; prompt-0007 | bad_instruction | Spec/skill silent on LLM wait ceilings; scenario was authored with 600 s. Operator cut to 60 s (prompt-0007). Patrick's call. |
| Recovery limit blocked task at `implementing` | events 68-72 | missing_capability | `head_sha` stuck at `258ec04` through local commits b895b21, eb07b7d, d354bb0 (`reconcile.py:9` only follows the PR head; no PR until event 89). Two `session_recovery` events (10, 35) on `(implementing, 258ec04)` hit the limit (`supervisor.py:191-196`). Also all verification entries before the PR carry `258ec04` (events 11-65). Operator fixed by hand via `state.update` + `cwf resume`. |
| `cwf handoff` unusable by the supervisor | operator context | infrastructure | By design ("requires a phase session"); not a failure. |
| llama-server started in the VM on 127.0.0.1; run invalidated | prompt-0006 (loopback, "no VM"), prompt-0007 correction | model_error (supervisor) | Operator-side error, wrong premise in the operator note. Cost ~16 min of build session 9 and a killed uitest (pid 57509). Not the coding agent. |
| uitest ignored SIGTERM, needed kill -9; Codex TUI Tab vs Enter | operator context | missing_capability | Harness issue (uitest signal handling) and Codex TUI queueing; not a skill defect. |
| Codex usage limit; session moved to codex-bravo manually | event 118 `session_usage_limit`, event 119 | infrastructure | Auto-resume was scheduled for 13:11; operator moved sooner. |
| Trial license ended on the VM | prompt-0010; operator note | infrastructure | Switched to debug build + `SCRIBULAR_SKIP_LICENSE=1`, uitest forwarding added (commit 3dee687). |
| Product license diagnostic changed to emit the operator-note sentence | operator note: "confirm the 'SCRIBULAR_SKIP_LICENSE set: access checks skipped' log line"; handoff 122 "fixed-event mapping of requested debug skip phrase"; `src-tauri/src/license/mod.rs:331,354` (commit 37e6dc8) | bad_instruction (operator note) + model_error | An operator note asked to confirm a log sentence the app did not emit; Codex edited product diagnostic codes (out of scope for a Cubic/Opus blocker commit). Skill does not say "if the line is not there, report it, do not change product code." Opus round 2 flagged it as a nit. |
| Cubic monthly cap, task `needs_human` | events 153-158 | infrastructure | Reviewer switched to `["agent"]` by hand at event 159 (operator). Product decision by Patrick. |
| `cwf set-reviewer` does not accept `agent` | `lib/cwf/cli.py:624` choices cubic/codex/knightwatch; config loader accepts agent (`knightwatch.py:83-88`); operator-guide lines 407 and 433 | missing_capability | Operator edited watcher-config by hand. |
| Operator rated Opus findings blocking to get Codex fixes | PR comments 7022334269 (CHANGES) vs first Opus read (APPROVE nits) | product_uncertainty | Patrick chose to have the 4 issues fixed; correct to treat as design decision. |
| `stale_head` polling for 11 min waiting for the agent review | events 214-225 | infrastructure | Agent review needs an operator-dispatched subagent; nothing automatic posts it. |
| AS1 tray interruption unverifiable | package AS1; event 183 | missing_capability | uitest Accessibility cannot trigger a native tray menu item (spec anticipated: `plans/uitest/05-native-surfaces.md:119-120`). |
| AS15 manual-only | spec; package | product_uncertainty | By design. |
| 4 timing-sensitive Rust failures with concurrent vitest | event 193 fail, 195 pass | model_error | Verify ran Rust and frontend suites in parallel; isolated rerun passed. Minor, recorded. |
| uitest failures at `37e6dc8` before passes | events 201-206 | bad_instruction | Likely 60 s waits and fixture drift after the 600 s to 60 s change; evidence at `runs/verify-ozAh42/acceptance.md`; cause per run not read. |
| cwf under-counts compactions | events 6-7 vs `report.json` compactions 0 | missing_capability | No code path writes a `compaction` event (`report.py:14` counts it; `hooks.py:173` only bumps `resumes`). |
| 14 h stall booked as `active` | `state.py:153`, report `active_seconds` 71719 | missing_capability | `verifying` with no live session is counted as active time. |

## Proposed changes (all apply to future runs only; installed skills pick them up in new sessions)

Paths below are relative to `~/Projects/clauffice/workflows/coding`.

**1. `skills/coding-task/SKILL.md` (Verification bullets, after the "Use a UI test tool" bullet around line 144): do not edit the product to satisfy a log-line check.** Evidence: operator note to "confirm" a log sentence; Codex then changed `license/mod.rs` diagnostic codes in 37e6dc8.

```diff
+- Confirming a log line or message from the task or operator note means reading it. If the app
+  does not emit it, record that as a finding and keep going; never change product code, event
+  names or log text only so a check or note phrase matches.
```

**2. `lib/cwf/hooks.py:173-175`: count compactions.** Evidence: events 6-7 are `resume source=compact` but `compactions` is 0.

```diff
         if source in ("resume", "compact"):
             _bump(changed, "resumes")
             events.append({"type": "resume", "source": source})
+        if source == "compact":
+            _bump(changed, "compactions")
+            events.append({"type": "compaction"})
```
Needs a unit test in `tests/` (compact SessionStart bumps `compactions` and emits `compaction`). Proposal until tested.

**3. `docs/operator-guide.md` line ~407 and ~433: document `agent` and the hand-editing requirement.** Evidence: operator switched `reviewers` to `["agent"]` by hand; CLI rejects it.

```diff
-`cwf set-reviewer cubic codex` (or any one or more of `cubic`, `codex`, `knightwatch`;
-Knightwatch requires `--knightwatch-login <login>`). The file holds no secrets.
+`cwf set-reviewer cubic codex` (or any one or more of `cubic`, `codex`, `knightwatch`;
+Knightwatch requires `--knightwatch-login <login>`). The `agent` reviewer is not a
+`set-reviewer` choice: edit `reviewers` in `watcher-config.json` and set `agent_reviewer_login`;
+an operator must post the `cwf:agent-review` comment on each new head (the wait shows `stale_head`
+until then). The file holds no secrets.
```

**4. `docs/operator-guide.md` operator_note section: say where the note's commands run and ask for observation.** Evidence: prompt-0006 said "no VM, use loopback" and a host log path; the agent ran in the VM and the supervisor started a server there.

```diff
+- `operator_note` is appended to every later phase prompt. State which machine the agents run on
+  (host or VM) and which paths they can read; ask them to *observe* log lines, not to make them
+  appear; remove or shorten the note once its action is done.
```

**5. Proposal only: `lib/cwf/reconcile.py` / `live.py`: follow local HEAD when no PR exists.** Evidence: `head_sha` stayed `258ec04` for four local commits, verification entries were recorded against it, and the recovery limit keyed on `(state, head_sha)` blocked the task (events 69-72). Direction: when `task.pr is None`, `live.snapshot` reports `git rev-parse HEAD` of the task worktree and `reconcile` emits `head_changed`. Changes review/gate state semantics (gate approval, `package_stale`), so it needs Patrick's design call and tests.

**6. Proposal only: `lib/cwf/state.py:153`: do not book time as `active` when the session ended without handoff and nothing is running.** Evidence: 845 min stall counted as active (19.9 h vs ~5.8 h real). Needs a waiting bucket for "stalled" and a supervisor notification when a session ends without handoff and no recovery starts in N minutes (cause of the 14 h stall is unknown, so unsafe to design blind).

**7. Proposal only: LLM wait ceiling for scenarios.** Evidence: two 600 s stalls and a 95 min verify (events 29, 52, 64-67). Direction: skill rule that real-model uitest steps use a 60 s ceiling and a stall is a defect to diagnose; project-specific (uitest README, Scribular docs), not a shared-skill change. Keep in Scribular `tools/uitest/README.md`.

**8. Proposal only: `skills/coding-task/SKILL.md` agent-review wait.** Evidence: events 214-225 polled `stale_head` for 11 min until the operator posted the review. Direction: a `needs_human`-style notification when an `agent` reviewer has no verdict on the current head after one poll. Design change; Patrick decides.

**9. Project docs (Scribular, not shared skills):** note in `tools/uitest/README.md` that uitest ignores SIGTERM (kill -9 needed) and cannot press tray items (AS1); add the host-`/completion` log check to `docs/remote-llm-uitest.md` as a host-side step.

## Applied by this cleanup

Nothing. No clauffice file was edited, no commit made, and no task state changed.
