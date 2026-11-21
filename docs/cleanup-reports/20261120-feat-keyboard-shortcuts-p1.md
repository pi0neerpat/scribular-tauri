# Cleanup report: 20261120-feat-keyboard-shortcuts-p1

**Outcome: merged.** PR #41 (keyboard shortcuts, part 1) merged at `365beea874a3cc50f58517bc32eab1482fa6d00b` on 2026-11-21 04:26Z; reviewed head `3bc95b0`. Technical review approved (agent reviewer, 0 outstanding). AS2 verified; AS1 and AS3 recorded as unverified manual items for Patrick. The task took about 9 h wall time but only ~5.7 h active; ~3.1 h was waiting on a human, mostly license, keychain and TCC gates (all `infrastructure`).

## Metrics

| Metric | Value |
|---|---|
| Active / human-wait / CI-wait | 20,551 s (5.7 h) / 11,178 s (3.1 h) / 966 s |
| Review rounds / fix pushes | 0 recorded (3 technical_review passes happened; see below) / 4 |
| Compactions / resumes | 0 / 0 |
| Interventions | 3 (2 usage-limit resets, 1 cleanup prompt) |
| Premature stops / stop blocks / false-ready | 0 / 0 / 0 |
| Demos | 2 (1 issue AS2 57 s, 1 happy-path 12.1 s at current head) |
| Tokens | Known: build 3.80M, verify 10.19M, cleanup 0.22M, total 14.2M across 7 of 18 sessions. The other 11 (all Codex) have no transcript usage: unknown. |
| Models | Build/verify/review: codex `gpt-6-sol`. Cleanup: claude `sonnet`. |

## Process review

- **Value per session.** Verify ran 9 times; three were pure gate retries that each lasted under 2 min and produced nothing: 03:49 (TCC missing), 03:53 (TCC missing), and the 21:15 release run (license). Build session #1 ended without a handoff after 2.2 min (`session_end ended_without_handoff` at 19:35:41) and was restarted. The 04:07 build and 04:15 verify (merge main, re-verify) were needed since the PR conflicted. Review session at 20:43 left no fix; the reviewer approved the head the build session fixed at 20:31.
- **Communication.** The verify handoff dropped one thing: the verified `...-verified` recording name could not overwrite an existing file, so a `-final` run replaced it (handoff note says so). Operator's debug-build recipe was carried in the handoff only after Patrick's 21:31 resume; the 21:15 verify session still tried a release build.
- **Time.** Longest stall: 230 min verify session (21:31 to 01:21) waiting on a keychain prompt, with a usage limit reset at 01:17 (intervention 2). Idle gaps: 147.5 min verify→verify (01:22 to 03:49, waiting for Patrick), 28.9 min review→verify (waiting for Patrick feedback). Smooth handoffs: build→verify and verify→review in the first pass (under 6 min gaps).
- **Environment gates (all `infrastructure`).** (1) License/trial: release builds show "Trial ended" and ignore `SCRIBULAR_SKIP_LICENSE` (event 21:29:22, blocked). (2) Keychain prompt for the database key (event 01:21:59, blocked). (3) Missing Accessibility and Screen Recording for tmux because the supervisor had been started with parent PID 1 (events 03:51:15 and 03:54:26, blocked twice). Fix applied by operator at 03:56 (restart in `cwf-watch`). The same note is already in global CLAUDE.md.
- **Tokens.** See metrics; verify dominates (10.2M, 72% of known). The final 10 min verify session alone was 6.6M, mostly cached input.
- **Parallel implementation.** The spec recommends no split; one agent per phase, as done.
- **PR hygiene.** One review-package comment (no duplicates). It is posted on head `3bc95b0` (not stale). The PR body has a `Recording:` line with an absolute local path in the body, not a recording filename at the very top; the path points outside the repo and the task dir copy exists. Evidence files cited in PR exist under `/tmp`, which may be cleared.
- **Demo evidence.** Final happy-path (12.1 s) starts with the app window open and shows File menu accelerators. The first happy-path recording showed only the license block (Patrick, 2026-11-20 21:15): the `infrastructure` license gate was the cause, and the agent had no way to know until the operator note. Remaining gap: the demo shows menu labels only; it cannot press the keys (AS1/AS3 manual).
- **Product review package.** Flags: (a) the recording block is titled "Recording", not "Proof of success", and has no mermaid user-journey diagram; (b) the section is named "Test it yourself", not "Human Verification Needed"; (c) AS1 and AS3 are each a single paragraph listing ~30 keys with no numbered steps, one action per step. Both items are legitimately manual, so listing them is right; the format is wrong.

## Classified items

| Item | Class | Evidence |
|---|---|---|
| Intervention 1: usage-limit reset | infrastructure | `logs/prompt-0001.txt`, event 20:15:55 |
| Intervention 2: usage-limit reset | infrastructure | `logs/prompt-0002.txt`, event 01:17:04 |
| Intervention 3: cleanup prompt (not a real intervention; task-notification style) | n/a (filtered) | `logs/prompt-0003.txt` |
| Blocked: release license gate | infrastructure | event 21:29:22 |
| Blocked: keychain prompt | infrastructure | event 01:21:59 |
| Blocked x2: tmux TCC permissions | infrastructure | events 03:51:15, 03:54:26 |
| Happy-path demo showed license block | infrastructure (license) plus `bad_instruction` (verify skill did not say to check the feature is visible before registering) | Patrick note, event 21:15:02 |
| Merge conflict with main after approval (cost 8 build + 10 verify min) | infrastructure (branch drift, not a workflow fault) | event 04:07:14 |
| Build session ended without handoff | model_error (unknown cause; no transcript) | event 19:35:41 |

## Proposed changes

1. **File:** `workflows/coding/skills/coding-task/templates/review-package.md`. **Change:** title the recording block "Proof of success" and add a mermaid journey diagram of what the demo shows; rename "Test it yourself" to "Human Verification Needed", number each step, one action each, only manual-only or agent-unverifiable items. **Evidence:** package comment on PR #41 (AS1/AS3 are 30-key paragraphs).
2. **File:** `workflows/coding/skills/verify/SKILL.md` (verify phase instructions). **Change:** before `cwf demo add --kind happy-path`, extract a frame and confirm the feature (not a gate dialog) is visible; if a gate shows, stop with needs_human. **Evidence:** Patrick's 21:15 feedback; 12.1 s clip replaced a license-only clip.
3. **File:** `watcher-config.json` template or cwf init default for projects that gate on a license. **Change:** set `operator_note` with the debug-build plus `SCRIBULAR_SKIP_LICENSE=1` recipe at task creation. **Evidence:** 21:29 block plus 2 h wait; the note arrived only after Patrick resumed.
4. **File:** `workflows/coding/skills/verify/SKILL.md`. **Change:** check `uitest doctor` (TCC and keychain) first, and block once instead of retrying a gate (two 0.5 to 1.7 min sessions each cost a human round trip). **Evidence:** events 03:51:15, 03:54:26.
5. **File:** `cwf demo add`/record naming. **Change:** refuse-or-suffix when the output name exists. **Evidence:** handoff note, `-verified` name could not be overwritten.
6. **File:** `Scribular/CLAUDE.md` (project doc). **Change:** note that release builds hit the trial gate on this Mac and uitest uses a debug bundle with `SCRIBULAR_SKIP_LICENSE=1`. **Evidence:** operator note, events above.

Fixes apply to future runs only; step 8 skipped (automatic mode).

## Next part

Spec part 2 (shortcut settings, global registration, Shortcuts pane; AS4 to AS7):
branch `feat/keyboard-shortcuts-p2`

```
cwf init --repo-dir "$HOME/Projects/Scribular/scribular-tauri-keyboard-shortcuts-p2" --spec "$HOME/Projects/Scribular/scribular-tauri-keyboard-shortcuts-p2/plans/2026-11-16-keyboard-shortcuts-spec.md" --part 2
```
