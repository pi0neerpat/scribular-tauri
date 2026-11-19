# Cleanup report: 20261119-feat-recording-p2-record-screen

All timestamps are UTC as in events.jsonl (event line numbers cited as `ev N`). Local = UTC-8.

## Summary

- Outcome: MERGED. PR 34 (pi0neerpat/scribular-tauri, branch feat/recording-p2-record-screen) merged 2026-11-19T18:33:31Z, merge commit 328915c071079e7f74adbab3dc63d99a62683926, final head a816750.
- cwf terminal state is `blocked` (not `done`): after the merge, the supervisor started a stray build session (ev 362-364, prompt-0043); the caller killed it and blocked the task (ev 369) because the state machine forbids implementing -> merged. Blocker text: "PR 34 merged (328915c) while task was product_feedback; supervisor missed the merge".
- Delivery took 16.8 h wall (01:48Z-18:36Z), of which ~10.5 h (07:18Z-17:51Z) was a stall (usage limit, then an unsent resume prompt). Machine work was ~4 h.
- Codex (gpt-6-sol) built and verified, Claude sonnet reviewed. 31 sessions, ~72.8M tokens (codex estimated), 71% of it in 12 verify sessions.
- The feature went through 5 technical-review approvals, 9 head changes and 9 fix pushes. The expensive part was AS3 (app audio with System Audio permission denied). The agent VM cannot exercise it, so it took three human host rounds. The root cause was only found from host evidence (silent exit 1 from AudioCapCLI --list-sources, fixed in a816750). Human validated AS3 on the host.
- cwf defects found: the hook logs every prompt in the checkout as an intervention (42 logged, ~12 genuine); a Monitor-expiry notification caused a false product_review_ready -> product_feedback transition; the supervisor merge check missed a merge in product_feedback; `cwf handoff` has no path for outside instructions; usage-limit auto-resume left its prompt unsent for ~9 h.
- Applied fixes (clauffice, future runs only): `f62cb96` supervisor follows a PR merged during `product_feedback` (`_close_if_pr_ended` + `product_feedback -> merged`); `179416c` hook ignores `<task-notification>` prompts.
- Spec change during review: AS8 live transcript revised (20 words per source, 20 active seconds before folding, 60 s trigger, commit 9a55c6f at ev 171); spec updated.

## Metrics (cwf report --json --task ...; session_tokens.py)

| Metric | Value |
|---|---|
| Terminal state | blocked (PR merged) |
| Active time | 51450 s (14.3 h; includes the 639 min stalled verify session) |
| Waiting on human | 7605 s (2.1 h) |
| Waiting on CI | 1428 s (0.4 h) |
| Review rounds (cwf counter) | 0 (counter not populated; events show 6 review sessions, 5 approvals or fix handoffs, see below) |
| Fix pushes | 9 |
| Compactions / resumes | 0 / 0 (cwf counter; 1 usage-limit auto-resume event, ev 277) |
| Premature stops / stop blocks / false ready | 0 / 0 / 0 |
| Interventions logged | 42 (only 12 are not supervisor-sent phase prompts; see classification) |
| Unverified manual scenarios at report time | 6 (cwf count; AS3 later validated by human on host) |
| Demos | 10 demo_added events, 1 kind (happy-path), 1 stale at report time |
| Cubic review packages posted | 3 `package_posted` events, all the same comment id 6999725699 (edited in place) |

Tokens per phase group (known sessions; codex counts are estimates; 1 of 31 sessions unknown: the last build, ev 369, killed after 0.6 min):

| Group | Sessions | Harness / model | Total tokens | Uncached input |
|---|---|---|---|---|
| build | 13 | codex / gpt-6-sol | 19,197,605 (12 known) | 875,608 |
| verify | 12 | codex / gpt-6-sol | 51,619,372 | 1,158,801 |
| review | 6 | claude / sonnet (exact id not recorded in events; session_start `model` is null) | 1,973,376 | 78 (+ 1.76M cached) |
| Grand total | 31 | | 72,790,353 | 2,034,487 |

Session model identifiers: build and verify `gpt-6-sol` (codex); review `sonnet` (claude, from token script; no full id available, unknown). Supervising out-of-band session: claude-opus-5-5 (per caller; not in events, tokens not in this table, unknown). `session_start.model` is null for the build/verify sessions (ev 364), so the model comes from the transcripts only.

## Process review

**Value per session.**
- Verify was the biggest cost (51.6M tokens, 12 sessions, 71%) and the most useful: it found the AS3 first-call "On a call" regression at 29f2dd0 (ev 16), and a stale-demo / moved-head problem at 07:09 (ev 258). Each verify drove the real signed app with uitest.
- Redundant rounds: (a) build 07:02 and verify 07:04 re-ran npm test/check/cargo test with "No recording code change was needed" (ev 247); (b) build 18:21 and verify 18:23-18:30 (6.3 min, 2.3M tokens) ran on a false product_feedback with no change (ev 339); (c) a816750 was verified twice (18:15 and 18:30) with no code change between; (d) build 03:57 -> verify 04:02 -> build 04:22 -> verify 04:23 for one test-only 2 s wait fix (b31ba4a), re-driving the whole app twice for a test change.
- Review (sonnet) sessions took 0.5-1.75 min each, ~0.2-0.5M tokens, and were cheap. The 03:14 and 03:56 reviews surfaced 10 and 3 valid Cubic threads, which led to 35bc5d7 and 853ca16. Good value.
- Overlap: 35ae182 (manual TCC preflight, pushed in the Claude session's window) was replaced by codex's classifier 382e2c9 (ev 205); the second fix did not address the real cause. Two sessions fixed the same finding with different guesses, and AS3 still failed on the host (prompt-0028).

**Communication / handoffs.** `handoffs/` does not exist in this task dir, so handoff notes were compared only through `handoff` events (ev 6-7 etc.). Findings:
- Smooth: build -> verify -> review handoffs carried commit hashes and test counts, with no re-verification gaps.
- Dropped: AS3 denied-permission behaviour was recorded "unverified" by the agents (6 unverified at report time), and the host failure reached the workflow only as pasted terminal text, so the evidence (empty stderr, exit 1) came from the human (prompt-0033/0035), not from any tool.
- The Claude session's push of 6f62d89 (license skip) was invisible to cwf; verify found it at ev 255/258 and sent the task back to build, wasting a verify round on a 13/13-step demo of the old bundle.
- The 17:51 outside instruction (prompt-0035) was typed into the codex pane because `cwf handoff` refuses outside a phase session (cli.py:252). It worked, but the intervention logger also treated it as a fresh hook prompt.
- Not in event evidence: the agents had no access to the host machine; the Claude session inspected a different local checkout once as if it were the host (caller fact, no event).

**Time.**
- Phase sessions 01:48Z-04:30Z: ~2.7 h wall, 5 review rounds, no idle gaps over 15 min (largest: 14.6 min verify->verify after blocker, ev 35-38: the shared uitest desktop was occupied by the note-style task, human unblocked it at 02:56).
- 04:30Z-05:54Z: 84.8 min waiting for the human (product_review_ready).
- 07:12Z verify started; 07:18Z codex usage limit (reset 08:39Z, ev 273); `session_stale` 07:46Z (ev 275); `session_usage_resume` 08:40Z (ev 277) but no prompt submitted until 17:47Z-17:51Z (prompt-0031..0035). Net stall 10.5 h, of which 1.4 h was the usage limit and ~9.2 h was the unsent resume prompt (cause of the delay between 08:40Z and 17:47Z is unknown from events; caller reports the prompt sat in the input box).
- 18:30Z product_review_ready -> merged 18:33:31Z: 3 min.
- Timezone note: the caller recalled 07:18-08:40 as local; events show those times in UTC (local 23:18-00:40). The 09:40 local resume matches 17:40Z.

**Tokens.** See table. Verify dominated; per-session peaks were 10.1M (verify 03:21, 30 tool calls) and 9.7M (verify 17:54). Unknown: the last build (no transcript usage), Opus supervising session, exact Claude review model id.

**Parallel implementation.** The spec (`plans/2026-11-16-recording-and-session-spec.md`, size section) calls for sequential PRs (PR2 = this one) and recommends no agent split. Implementation was done by one codex build agent per round (tool counts 0-12, no subagents recorded). Nothing to flag.

**PR hygiene.** (`gh pr view 34 --comments`)
- Single cwf review-package comment, edited in place across heads (3 `package_posted` events, same comment id): no duplicates, good.
- The PR description begins with `Recording: /private/tmp/scribular-p2-verify-evidence/...mp4` (demo.py:163): a local VM path nobody else can open, and the host machine where Patrick tests cannot see it. Problem: the recording line points at a file that does not exist for the reviewer.
- Cubic summaries (4 comments) each end "All reported issues were addressed"; `<!-- cwf:agent-reply -->` replies exist. The Cubic quota banner (90-91% of monthly reviewed lines) is a warning to Patrick, not a cwf fault.
- Evidence files exist locally under /private/tmp/scribular-p2-*-evidence/ (10 demo_added events; all named happy-path). tmp is volatile.

**Demo evidence.** Only a happy-path recording (in-room Start/Stop, 11-13 steps) was registered, each time under the same filename, for the unchanged feature surface. The AS3 unresolved issue was recorded at 02:16 (`recording-p2-first-call-retry-issue`, ev 12) but never registered as a demo. AS3 denied-permission cannot be captured in the VM at all (System Audio TCC prompt, host-only), so no demo shows the actual bug and fix; human validation on the host is the only evidence. Recordings were not paced for a reviewer (UI-test automation speed); not verified in this report.

## Classified items

Prompts 0001-0015, 0017-0021, 0023-0024, 0027, 0029, 0036-0038, 0040-0041, 0043 are supervisor-sent phase prompts (30 of 42), not interventions: see proposal 1. The 12 items below, plus other findings, are classified.

| Item (prompt / event) | Class | Evidence |
|---|---|---|
| 0016 AS3 failure on host: no Retry/Reopen buttons with System Audio denied (ev 166-167) | missing_capability | Agents cannot exercise denied System Audio on the host; verify 06:19 passed 12/12 on VM (ev 195) while host failed. |
| 35ae182 -> 382e2c9: preflight replaced by classifier, AS3 still failed (ev 178, 205; prompt-0028) | missing_capability | No host probe tool; the cause (silent exit 1, empty stderr) needed Patrick's terminal output (prompt-0033). Guessing twice before measuring. |
| 0022 "dev build complaining about license expired" (ev 241-242) | missing_capability | No documented/debug way to test a built app without a valid license; fixed by 6f62d89 (SCRIBULAR_SKIP_LICENSE not honoured per prompt-0025). |
| 0025/0028/0030 AS3 retests: Terminal app permission disabled on host | infrastructure | macOS permission state on the host (prompt-0030 "terminal app permissions were disabled"). |
| 0026, 0031 "AS9 verified" / "AS8 verified"; 0033 host probe output | product_uncertainty | Human-only host verification; not a workflow failure. |
| 0032 "theres gotta be an easier way" | missing_capability | Patrick had to run the probe by hand on the host; no cwf command to give him one command and relay output. |
| 0034/0035 push cwf back to build, instruction typed into codex pane | missing_capability | cli.py:252 "handoff requires a phase session"; no human/outside handoff path. |
| 6f62d89 pushed directly by the Claude session (ev 255, 258) | model_error | skill says verify pushes the verified head; an outside session pushing moved head and sent verify -> build, wasting the 07:09 demo run (stale demo). |
| 07:18Z usage limit (ev 273) | infrastructure | codex quota, reset 08:39Z. |
| 08:40Z resume prompt unsent until 17:51Z (ev 277; no prompt log between 0030 and 0031) | missing_capability | supervisor.py:139-141 types text and Enter back-to-back with no check that Codex submitted it; no new event or notify when work does not resume. |
| 0039 Monitor-expiry `<task-notification>` -> product_review_ready -> product_feedback (ev 329-330) | missing_capability | hooks.py:88 treats every prompt as feedback; false transition made build 18:21 and verify 18:23-18:30 run for nothing (ev 339 note says so). |
| 0042 "AS3 is validated PR is merged" -> product_feedback at 18:33:52 (ev 359-364) | missing_capability | PR merged 18:33:31 while task was product_review_ready; the hook flipped the state before the supervisor polled; the old `_close_if_pr_ended` excluded product_feedback, so build started 18:33:58 (prompt-0043). Fixed by the caller (see Summary). |
| 02:41 blocked: shared uitest desktop occupied by another task (ev 35-38) | infrastructure | single-run lock held by note-style task PID 49534; human resumed 02:56. |
| Cubic reviewer network errors, 6 events (ev 57-61, 224-228) | infrastructure | reviewer_network_error x3 per package; polls recovered. |
| Premature stops / false ready | none | cwf counters 0; no `false_ready` entries. |

Correction to the caller's note 6: at merge time the task was in product_review_ready (ev 358), not product_feedback; the human's prompt-0042 (21 s after merge) moved it to product_feedback before the supervisor polled, and the old check did not cover that state.

## Proposed changes (against ~/Projects/clauffice/workflows/coding/)

Ranked by value. [S] = small enough to apply now.

1. [S] Hook: ignore task-notifications and supervisor-sent phase prompts as interventions and feedback (fixes 0039 and the 42-vs-12 count). `lib/cwf/hooks.py` `user_prompt`: — APPLIED in part (179416c): `<task-notification>` prompts are skipped. Filtering cwf's own phase prompts is still a proposal.
```diff
 def user_prompt(directory, data, harness):
     prompt = data.get("prompt") or ""
+    if prompt.lstrip().startswith("<task-notification>"):
+        return None  # background-task notices are not human input
     pause = bool(PAUSE.match(prompt))
```
   Phase prompts: skip when text starts with "Use the coding-task skill for phase " (prefix in sessions.py:117 area). Add a unit test in tests for both.
2. [S, APPLIED f62cb96] Merged PR ends the task when merged during `product_feedback` (the observed case). Widening to `implementing`/`verifying` was rejected: it conflicts with the tested guard that refuses `verifying -> merged`, and it would add a `gh` call to every build tick.
3. [S] Usage-limit resume: type text, wait, press Enter, verify the pane left the input box, else retry once and notify. `supervisor.py:139-141`:
```diff
         subprocess.run(["tmux", "send-keys", "-t", name, "-l", "Usage limit reset. "
                         "Continue the current phase from where you left off."])
+        time.sleep(1)
         subprocess.run(["tmux", "send-keys", "-t", name, "Enter"])
```
   plus on the next tick, if `parse_codex_pane` still shows the resume text in the input (`idle` true) after 2 min, notify("resume prompt not submitted") and send Enter once more. First part is a one-liner; the verify/notify part is medium.
4. Human handoff path: `cwf handoff --next build --note ... --task <id> --as-human` (or `cwf feedback --task <id> "text"`) allowed outside a phase session in states product_review_ready, product_feedback, verifying (stalled). Replaces typing into the codex pane. Medium (cli.py:249-252 plus actor=human, event type `human_handoff`). Not small.
5. [S] coding-task skill: add to the verify/outside-session rules: "Only the verify phase pushes to the PR branch. If you (supervising session, human helper) must push, run `cwf pause` first or hand the commit to build, otherwise verify sees head moved and restarts." Plus: "Features whose acceptance needs host-only TCC/permission state: record the exact probe command for Patrick once, and add 'probe output' as the required evidence; build must not guess a fix without a probe result." (skills/coding-task/SKILL.md near lines 54-60 and the manual-only scenario rule.)
6. [S] Report counters: `cwf report` should exclude `phase prompt` and task-notification interventions, and populate `review_rounds` from `reviewer_poll`/review session starts (report.py). Medium for the review rounds, small for filtering.
7. Demo/PR: do not write a `Recording: /private/tmp/...` local path into the PR body (demo.py:163); either upload (gh asset/GitHub release) or write `Recording: local only, see <task>/demos` and register one demo per unique scenario (the AS3 failure recording was never registered). Medium.
8. Verify scope on test-only or no-change heads (b31ba4a, 07:04, 18:23): allow build to hand off straight to review ("no code change") when the diff touches only tests or is empty, saving ~2-9M tokens per round. Needs design (verification gate), so a proposal for Patrick, not a cleanup edit.

Not changed: tests, Knightwatch review, human approval and credential boundaries were not weakened; none of these proposals touches them.

Fixes apply to future runs only; sessions already running keep the behaviour they started with.
