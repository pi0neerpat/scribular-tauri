# Cleanup report: 20261123-feat-keyboard-shortcuts-p3

**Outcome: merged.** PR #59 merged at `76d6bd21cbfa1e520221045724e3a484777f29ab` (2026-11-25T23:47Z), head `a716563`. Spec part 3 of 3 (`plans/2026-11-16-keyboard-shortcuts-spec.md` lists PR1-PR3 only), so there is no next part.

**Summary.** The feature shipped, but the run took about two days of wall time. Most of that was waiting on blockers, not work. Of 27 `needs_human` events, about 20 trace to four causes: tmux lacking macOS TCC grants, a `cwf screenshot` bug, a "need a microphone" stop that repo CLAUDE.md forbids, and a foreign keychain prompt. AS10 (tray-start fallback) was never verified natively. Patrick waived it. The review package misses several template requirements, and the PR body still says screenshots failed after they were fixed.

## Metrics

| Metric | Value |
|---|---|
| Active time | 84,539 s (23.5 h) |
| Waiting on human | 94,187 s (26.2 h) |
| Waiting on CI | 1,566 s (26 min) |
| Fix pushes | 9 |
| Review rounds (cwf counter) | 0 (the counter looks wrong: Cubic raised 14 threads, then 5 more) |
| Compactions / resumes | 0 / 0 |
| Premature stops / stop blocks / false ready | 0 / 0 / 0 |
| Interventions | 3 (all classified below) |
| `needs_human` events | 27 |
| Unverified manual items | 2 |
| Demos | 1 happy-path, 0 stale |
| Sessions | 36 |

### Models and tokens (`session_tokens.py`)

| Phase / harness | Sessions | Tokens |
|---|---|---|
| build / codex (gpt-6.2-sol) | 7 | 40.8 M |
| build / claude (opus) | 1 | 0.8 M |
| verify / codex (gpt-6.2-sol: 4 sessions; gpt-6.1-terra: 13) | 17 | 57.8 M |
| verify / claude (sonnet) | 3 | 1.9 M |
| review / codex (gpt-6.2-sol) | 1 | 0.6 M |
| review / claude (sonnet) | 5 | 1.7 M |
| cleanup / claude (sonnet), partial | 1 | 0.6 M |
| **Total** | 35 | **104.2 M** |

Codex counts are estimated. The 2 sessions with no transcript usage are unknown and excluded: one codex verify session at 17:57 on 11-24, and one claude session at 22:52 on 11-25 that logged 0. The script's own "Phase totals" footer listed only `build` and `other`. It under-reports, so these rows come from its per-session lines. See proposal 7.

## Process review

**Value per agent session.**
- The 17 codex verify sessions are the cost centre (57.8 M of 104 M tokens). Eleven ran under 3 minutes and ended in a handoff that only restated a blocker.
  - Five were tmux TCC blocks: `session_start` at 18:00, 18:02, 18:10, 18:11 and 20:15 on 11-24, each followed by `needs_human`.
  - Two were `cwf screenshot` blocks at 20:53 and 21:47.
  - Two were microphone blocks at 22:32 and 23:37.
- One verify session ran 1006 min (01:11 to 17:57 on 11-24). It sat idle after a usage limit and a stale timeout, and ended without a handoff.
- The review phase ran 7 sessions. Four of them (23:33, 23:35, 23:37, 23:41 on 11-25) re-reviewed the same head `a716563`. The first three produced review->review handoffs until cwf blocked the loop at 23:38:13 ("3 in a row"). That loop needed Patrick's AS10 waiver at 23:40:31 to clear.
- Two verifies re-ran what build had already proved: `npm test`, `check` and `build` were run again on `197fd94` and on `a716563`. Both were cheap and could not be skipped.
- Build/verify ping-pong on harness bugs worked: `7f31431` and `80a0b06` (new scenarios), `5776167` (AS9 launch blocker), `e35312e` (long speech fixture).

**Communication.**
- The task has no `handoffs/*.md`. The handoffs are `reports/*.ingested.json` (32 generations), so I checked prompts and `events.jsonl` instead of handoff-vs-next-session text.
- Dropped items:
  - The resume reason for the mic block (`cwf resume --reason`) never reached the agent. The microphone request came back twice (22:32, then 23:37). It stopped only when `watcher-config.json` `operator_note` was set (cleanup-notes item 4).
  - The first prompt said "AS9 mic fixture", but repo CLAUDE.md says App audio + `play`. The agent did not apply that rule on its own.
  - The PR body (written 00:53 on 11-25) says "Screenshot capture failed with Errno 2". The screenshot fix was verified at 21:53 on 11-24. The posted package still says "cwf screenshot failed 3x" for AS9.

**Time.**
- Blocked gaps:
  - 18:00 to 20:15 on 11-24, tmux TCC.
  - 20:53 to 21:44, screenshot.
  - 22:32 to 23:28 and 23:37 to 23:52, microphone.
  - 01:12 on 11-25 to 22:52 (21.7 h), a foreign SecurityAgent prompt, then codex-bravo exhausted.
- Usage limits hit twice, at 21:48 on 11-23 and at 01:34 on 11-24.
- Stale timeouts fired twice: 22:52 on 11-23 and 02:02 on 11-24.
- Handoffs that went smoothly: build to verify on 11-24 (01:02, 01:11, 20:28 to 20:40) and the Cubic fix cycle on 11-25 (01:00 to 01:12 and 23:12 to 23:25).
- Environment gates that stalled verify, all `infrastructure`:
  1. tmux started by the LaunchAgent has no TCC grants (events 18:00, 18:02, 18:10, 18:12, 20:15 on 11-24).
  2. Sandboxed tmux anchor: sessions inside it could not see WindowServer windows (cleanup-notes item 2).
  3. An Agent Token Monitor keychain prompt blocked uitest input (event 01:12 on 11-25).
  4. Control Center covered the tray status item (AS10 click, 3 runs).
  5. codex usage limits.

**Parallel implementation.** The spec's Size section says "L, about 7-10 days" and lists a PR split, but recommends no parallel agents. The build ran as one agent session at a time. Nothing to flag.

**PR hygiene.** There is 1 comment, the product review package, posted once (no duplicates). The PR body is partly stale. It lists "Historical native scenario reruns include failures" and "Screenshot capture failed with Errno 2". Both described the old state. The evidence logs exist under `<task dir>/logs/`.

**Demo evidence.** There is one happy-path demo, `2026-11-25-...-happy-path.mp4`, 55 s long. It was recorded from `AS10-stop-recovery-evidence.uitest`, so it shows stop-failure recovery. It does not show the headline behaviour: background chord start/stop with another app frontmost. Dead lead-in is small: the first action is `launch` and the first `expect` is 0.4 s later. The 11-24 recordings are earlier versions of the same demo. Not flagged for env-gate content.

**Product review package.** Checked against the review-package template rules:
- The recording block is titled "Recording", not "Proof of success". Missing.
- There is no mermaid user-journey diagram. Missing.
- "Test it yourself" is not named "Human Verification Needed". The AS9 item lists a step that uitest already did (background start/stop, note creation). Only the notice banners were unobserved. The AS10 item is a waived scenario, not a human-only one.
- Steps are numbered but compound (step 3 is a four-way check list).
- The recording path is a bare absolute path on a local disk, not a link a reviewer can open.

## Classified items

| # | Item | Class | Evidence |
|---|---|---|---|
| 1 | Intervention: `prompt-0001` "Usage limit reset. Continue." | infrastructure | `logs/prompt-0001.txt`; `session_usage_limit` 21:48 on 11-23 |
| 2 | Intervention: `prompt-0002` "finish now... record mic delta as superseded" | model_error (mic stop) + missing_capability (`resume --reason` not injected) | `logs/prompt-0002.txt`; cleanup-notes item 4; blocks 22:32 and 23:37 on 11-24 |
| 3 | Intervention: `prompt-0003` | filtered (this cleanup session's own prompt) | `logs/prompt-0003.txt` |
| 4 | 5 TCC blocks (verify needs_human) | infrastructure | events 18:00, 18:02, 18:10, 18:12, 20:15 on 11-24; cleanup-notes item 1 |
| 5 | Sandboxed tmux anchor | infrastructure | cleanup-notes item 2 |
| 6 | 2 `cwf screenshot` blocks | missing_capability (tool bug) | events 20:53 and 21:47 on 11-24; cleanup-notes item 3 |
| 7 | 2 microphone blocks | model_error | repo CLAUDE.md; events 22:32 and 23:37 on 11-24 |
| 8 | Foreign keychain prompt block, 21.7 h gap | infrastructure | event 01:12 on 11-25 |
| 9 | Review->review loop x3 on `a716563` | bad_instruction (review phase cannot hand off "waived/unverifiable, approve") | transition 23:38:13 |
| 10 | AS10 tray-start never native-verified; waived | product_uncertainty (Patrick's call) + infrastructure (Control Center overlay) | `verification_waiver` 23:40:49 |
| 11 | Two stale-session timeouts, one verify at 1006 min | infrastructure | `session_stale` 22:52 on 11-23 and 02:02 on 11-24 |
| 12 | Stale PR-body claims; package missing "Proof of success", mermaid, "Human Verification Needed" | bad_instruction | PR #59 body and comment |
| 13 | Demo shows stop recovery, not the headline chord path | bad_instruction (demo selection) | `recordings/*.json` scenario field |

No premature stops, no false-ready reports.

## Proposed changes (not applied; automatic mode skipped step 8)

1. **File:** `workflows/coding/skills/coding-task/SKILL.md` (verify guidance).
   **Change:** say that a tool failure on one artifact (screenshot) or a missing physical device is `--unverified` plus continue, not `needs_human`, when an App-audio or fixture alternative exists. It must also say to read the repo CLAUDE.md test rules before asking for hardware.
   **Evidence:** items 6 and 7, 4 of 27 needs_human events (cleanup-notes proposal 4).
2. **File:** `workflows/coding/lib/cwf/sessions.py` (near line 199).
   **Change:** put the latest `cwf resume --reason` text into the next phase prompt, or document that `operator_note` is the only channel.
   **Evidence:** item 2; the mic block recurred after a resume reason was given.
3. **File:** `workflows/coding/lib/cwf/screenshot.py`.
   **Change:** commit the `ObjC.castRefToObject` fix to `_window_id` and the 10x0.5 s window wait, with a unit test that mocks osascript output. Push to Forgejo.
   **Evidence:** item 6. The fix sits uncommitted in `~/Projects/clauffice` on `main` (cleanup-notes item 3).
4. **File:** the LaunchAgent / `cwf watch` start path.
   **Change:** refuse to start the tmux server from the LaunchAgent. Require a terminal-started anchor, or launch tmux through a TCC-granted wrapper. `uitest doctor` failure text should name this cause.
   **Evidence:** item 4, 5 blocks, 2+ h lost.
5. **File:** `workflows/coding/skills/review/SKILL.md` (or the review-phase prompt).
   **Change:** when the only open items are waived or unverifiable scenarios, hand off `ready` with the item listed as unverified, instead of another review->review handoff.
   **Evidence:** item 9, 4 sessions on one head.
6. **File:** `workflows/coding/skills/coding-task/templates/review-package.md`.
   **Change:** require a "Proof of success" title and a mermaid journey diagram for the recording block. Rename "Test it yourself" to "Human Verification Needed", list only manual-only or agent-unverifiable items, one action per numbered step. Require a regenerated PR body when evidence changes, and a recording reference a reviewer can open.
   **Evidence:** items 12 and 13.
7. **File:** `~/.claude/skills/cleanup/scripts/session_tokens.py`.
   **Change:** make "Phase totals" cover all phase groups (it printed only `build` and `other`).
   **Evidence:** the token section above.
8. **File:** `~/.claude/CLAUDE.md` (global).
   **Change:** the "run supervisor in tmux `cwf-watch`; PPID 1 means orphan" guidance is outdated now that LaunchAgent `com.clauffice.cwf-watch` owns the supervisor.
   **Evidence:** cleanup-notes item 5.
9. **File:** `Scribular` repo `tools/uitest/README.md`.
   **Change:** note that Control Center can cover the status item, and which `uitest` command clicks an obscured tray item.
   **Evidence:** item 10, 3 failed runs.

These edits apply to future runs only. Skills load at session start.

## Human Verification Needed

- AS10: tray-start fallback was waived by Patrick. It has no native run.
- AS9: macOS notification banners were not observed by uitest.

## Next part

None. Spec part 3 of 3.
