# Cleanup report: 20261121-feat-session-coach-p2

## Summary

PR 53 ("Session Coach PR2: note conversation and guarded acceptance") merged on 2026-11-23T18:56:11Z as `3d7d0c03fd810c0d5c589d1f9f19ac70af485a07` (base `main`). The task took about 2.5 days of wall time, almost all of it agent time plus waiting on Codex usage limits and the Cubic reviewer. Three things cost the most: eight Codex usage-limit stops, a Cubic reviewer that never answered for a day, and review that did not converge in 3 rounds. The PR body lacks the required review package structure. No `deployed_commit` applies (not a cwf self-task).

## Metrics

| Metric | Value |
|---|---|
| Terminal state | cleanup (PR merged) |
| Head at cleanup | a0f7570f841a1dc6f1d4e8cd50d67431471c4ceb |
| Active time | 163,758 s (45.5 h) |
| Waiting on human | 11,782 s (3.3 h) |
| Waiting on CI/reviewer | 49,324 s (13.7 h) |
| Review rounds (cwf count) | 0 (events show "did not converge after 3 rounds", event 1740) |
| Fix pushes | 7 |
| Compactions / resumes | 0 / 0 |
| Premature stops / stop blocks / false-ready | 0 / 0 / 0 |
| Interventions | 3 (2 usage-limit resumes, 1 cleanup prompt) |
| Demos | 2 live (1 happy-path, 1 issue AS10), both stale; 11 `demo_added` events |
| Unverified manual scenarios | 0 |
| Sessions | 31 (events 3..1791): Codex gpt-6.2-sol for build/verify/review through 2026-11-23T15:02; Claude sonnet (review, verify, review, cleanup) and opus (build) for the last 30 min |
| Tokens (known only) | 149.3M total: verify 110.3M, build 37.7M, review 1.17M, cleanup 0.14M. 21 of 31 sessions have usage data; 10 Codex sessions between 2026-11-21T23:34 and 2026-11-23T04:17 report none, so the real total is higher |

## Process review

**Value per agent session.**
- Many build/verify pairs were short and mechanical. Examples: build sessions of 6.1, 5.5, 3.3, 5.5, 6.0 min each followed by a verify session (events 1287-1292, 1337-1342, 1389-1394, 1631-1643). These look like one-finding fix passes, which is the intended loop.
- Verify sessions are the cost: 110.3M tokens, 8 of 11 known. The 2026-11-21 verify alone took 303 min and 19M tokens, and later verifies re-ran full suites for each fix head (`verify-3f34196`, `verify-9e83756`, `verify-b14ee25`... in `recordings/`). The same suites (`npm test`, `npm run check`, `cargo test --lib`, audits) were re-run at nearly every head; the stale marks in `cwf status` show 14+ verification rows pinned to an older head 208f5b3.
- Review sessions were mostly empty: Codex review sessions of 1.7-2.4 min with 0-620K tokens (events 1328, 1380, 1581) and two review sessions of 567 and 221 min waiting for the reviewer.

**Communication.** `<task dir>/handoffs/` does not exist; handoffs are the `reports/*.ingested.json` files (27 handoff events). I did not diff each against the following session. Unknown: dropped items. Observable: the cleanup prompt was the only human-authored prompt text after two identical "Usage limit reset" resumes.

**Time.**
- Longest sessions: build 270 min (2026-11-21), verify 303 min, review 567 min (reviewer silent), build 901 min (events 1440-1445, spanning a usage-limit wait), verify 289, 238, 249 min.
- Largest idle gap: 203.8 min verify→review (2026-11-23T15:02 to 18:26) after the "did not converge" block (event 1740).
- Stalls, all `infrastructure`: eight "codex usage limit hit; auto-resume" `needs_human` events (events 6, 22, 57, 1349, 1400, 1444, 1545, 1648); six `session_stale` verify timeouts (events 23, 1296, 1350, 1401, 1546, 1649), each following a usage limit so the stale session was the limit, not a hang; 617 `reviewer_network_error` events (463 on 2026-11-21, 148 on 2026-11-22, 6 on 2026-11-23) and "technical reviewer has not responded" (event 940).
- License/trial gate: the release build showed "Trial ended" on this Mac. `infrastructure`. Handled by the operator note (debug bundle with `SCRIBULAR_SKIP_LICENSE=1`, in the cleanup prompt). Not recorded in `watcher-config.json`, so it only reached the late sessions. Proposal 2 below.

**Tokens.** See metrics. Codex counts are estimated from cumulative `token_count` deltas; 10 sessions unknown.

**Parallel implementation.** The spec (`plans/2026-11-16-session-coach-spec.md`, part 2) did not recommend splitting implementation across agents. One agent per phase, no subagent use found.

**PR hygiene.** `gh pr view 53` shows 0 PR comments, so there are no repeated comments. The PR body has a `Recording:` line with an absolute local path near the top (a path the reviewer cannot open from GitHub) and no "Proof of success", mermaid diagram or "Human Verification Needed" section (0 matches). The Cubic summary block is appended and current for a0f7570.

**Demo evidence.** Four recordings exist (2026-11-21, 11-22, 11-23 happy-path, 11-23 issue AS10); `cwf demo list` marks both live demos stale because the head moved after recording. I did not play the videos, so dead lead-in is unknown. The issue demo is named for AS10 as the spec requires; a recording of a native-test happy path was re-recorded six times (`demo_added` events 41, 1319, 1374, 1434, 1482, 1572, 1625, 1722, 1773).

**Product review package.** Missing "Proof of success" title and mermaid diagram in the PR body. `coding-task/templates/review-package.md` has no match for "Proof of success", "Human Verification" or "mermaid", so the template itself lacks the requirement.

## Classified items

| # | Item | Class | Evidence |
|---|---|---|---|
| 1 | Intervention 1: "Usage limit reset. Continue the current phase" | infrastructure | `logs/prompt-0001.txt`, event 1353, usage limits at events 1349/1400 |
| 2 | Intervention 2: same resume prompt | infrastructure | `logs/prompt-0002.txt`, event 1404, limit at event 1400/1444 |
| 3 | Intervention 3: cleanup prompt | not an intervention (task-notification style cleanup dispatch) | `logs/prompt-0003.txt`, event 1793 |
| 4 | Reviewer silent, 617 network errors, `needs_human` "technical reviewer has not responded" | infrastructure | events 48..., 940; 463 errors on 2026-11-21 |
| 5 | Review did not converge after 3 rounds, 4 open threads | model_error / unknown | event 1740 (head 9e83756); the thread contents were not read; the fix PR commits (b14ee25, a0f7570) resolved it |
| 6 | Release license gate "Trial ended" | infrastructure | operator note in `logs/prompt-0003.txt` |
| 7 | PR body lacks Proof of success / mermaid / Human Verification | bad_instruction | template has no such section; 0 matches in PR body |
| 8 | Premature stops, stop blocks, false-ready | none | report counts are 0 |

## Proposed changes (not applied; automatic mode skips step 8)

1. **File:** `workflows/coding/skills/coding-task/templates/review-package.md`
   **Change:** Title the recording block "Proof of success" with a mermaid user-journey diagram; name the test section "Human Verification Needed" with numbered one-action steps; forbid local filesystem paths on the first lines of the PR body.
   **Evidence:** PR 53 body has 0 matches for those headings and a `Recording: ~/.local/state/...` line at the top.
2. **File:** `workflows/coding/` default `watcher-config.json` generation (or `cwf init` docs)
   **Change:** Let `cwf init` accept an `operator_note` for known environment gates so license/trial instructions apply from the first session, not just from the late ones.
   **Evidence:** the "Trial ended" gate note appears only in `prompt-0003.txt`.
3. **File:** `workflows/coding/` verify phase prompt
   **Change:** Re-run only suites whose `paths` match the files changed since the last verified head; say so in the handoff.
   **Evidence:** verify sessions used 110.3M of 149.3M known tokens; verification rows are repeated per fix head.
4. **File:** supervisor (`cwf watch`) reviewer-wait logic
   **Change:** After N consecutive `reviewer_network_error` events, stop polling and raise one `needs_human` instead of 617 error events over two days.
   **Evidence:** events 48 onward; counts per day above.
5. **File:** `workflows/coding/` session usage accounting
   **Change:** Record token usage for every Codex session (10 of 31 have none).
   **Evidence:** `session_tokens.py` output "no transcript usage found" for events 943-1445.

## Docs and follow-up

- Docs: `docs/session-coach-conversation.md` in the docs/report PR.
- Ignored evidence: `runs/` logs and JSON copied to `<task dir>/evidence/`; recordings stay in `<task dir>/recordings/`.
- `spec.part` is 2; the spec's next part is PR3 (the one-time style offer). Next-part command:

```
cwf init --repo-dir "$HOME/Projects/Scribular/scribular-tauri-session-coach-p3" --spec "$HOME/.local/state/clauffice/workflows/pi0neerpat__scribular-tauri/20261121-feat-session-coach-p2/session-coach-spec.md" --part 3
```

Branch name: `feat/session-coach-p3`. The spec is copied into the task directory so it survives the clone's deletion.
