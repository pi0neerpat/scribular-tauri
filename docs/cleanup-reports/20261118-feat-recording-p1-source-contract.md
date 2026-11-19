# Cleanup report: 20261118-feat-recording-p1-source-contract

## Summary

PR #32 (Recording PR1, the Rust source contract) merged as `bdd0f5c704dd70bd2a05d2a2c38e3350f1e22cf3`. There were 12 Codex sessions, all `gpt-6.2-sol`, with no interventions, premature stops or false-ready reports. Two avoidable costs came from the workflow, not the model:

1. A stale supervisor did not honour `package_file` handoffs, which repeated a review session. The user noticed "multiple review agents."
2. The PR body never got its `Recording:` line, because review sessions cannot run `demo pr-line --apply` and the supervisor did not do it for them.

Fix 2 and a token-script fix are committed in clauffice `db27497` (branch `feat/cwf-multi-reviewer`). Fix 1 remains a proposal.

## Outcome

- PR: https://github.com/pi0neerpat/scribular-tauri/pull/32, merged at `bdd0f5c`, approved head `e2886f7`.
- Scope: AS1, AS4–AS6 and AS18 (PR1 slice). AS5 native meters are left for PR2, and AS17 for PR4 (manual).

## Metrics

| Metric | Value |
| --- | --- |
| Active time | 78 min |
| Waiting on human | 27 min |
| Waiting on CI | 21 min |
| Sessions | 12 (build 4, verify 4, review 4) |
| Fix pushes | 3 |
| Compactions / resumes | 0 / 0 |
| Interventions / premature stops / false ready | 0 / 0 / 0 |
| Unverified manual checks | 2 (AS5 native, AS17) |
| Demos | 3 (1 happy path, 2 stale issue demos) |
| Tokens (estimated, Codex deltas) | 34.3M total, 33.0M cached, 1.2M uncached; build 14.8M, verify 17.2M, review 2.3M (12/12 sessions known) |

## Process review

**Value per session**

| # | Phase | Wall | Result |
| --- | --- | --- | --- |
| 1 | build | 19.8m | Initial implementation |
| 2 | verify | 8.9m | Found a real onboarding-copy defect |
| 3 | build | 2.1m | Fixed it |
| 4 | verify | 7.9m | Opened the PR |
| 5 | review | 2.2m | Raised 3 Cubic P2s |
| 6 | build | 7.9m | Fixes |
| 7 | verify | 8.5m | |
| 8 | review | 1.7m | Raised 2 P2s |
| 9 | build | 11.7m | Fixes |
| 10 | verify | 7.9m | |
| 11 | review | 3.4m | Approved; package handed off |
| 12 | review | 1.7m | **Redundant**: same head, caused by the stale supervisor |

All sessions except #12 changed the outcome. Verify used the most tokens (17.2M). Most of that is re-reading cached context; uncached input was only 0.45M.

**Communication.** The handoffs carried the Cubic findings and the PR1 scope operator note through every round. The package correctly assigns AS5 native and AS17 to later PRs. One gap: the package body includes a stray `<!-- cwf:agent-reply -->` marker, which is cosmetic.

**Time.** Gaps between sessions were under 5 minutes, all waiting on Cubic between verify and review. The 0.32-minute review→review gap is the duplicate session.

**Parallel implementation.** Not applicable. The spec orders PR1–PR4 sequentially.

**PR hygiene.** There is one package comment and no duplicates. Cubic left 3 COMMENTED reviews. The PR body has **no `Recording:` line**.

**Demo evidence.**

- The 23-second happy-path recording shows onboarding and Recording settings. That is reasonable, since PR1 is mostly Rust with little visible UI.
- Two stale issue demos are registered with the same path (a duplicate registration).
- The recording was written to the main checkout's `runs/` instead of the task clone.

## Classified items

| Item | Class | Evidence |
| --- | --- | --- |
| Redundant review session #12 | infrastructure / missing_capability | The supervisor had loaded cwf code from before the `package_file` handoff commit. It kept restarting review until it was restarted by hand. |
| Missing PR `Recording:` line | missing_capability | `cli.py:344-351` `_read_only` blocks `demo pr-line --apply` in phase sessions. `supervisor._post_pending_package` posted the package only. |
| Token script found 0/12, then 7/12 sessions | missing_capability (cleanup tooling) | It ignored `watcher-config.codex_home` and the local-date Codex day folders. |
| Recording outside the clone | bad_instruction (minor) | The path is under `scribular-tauri/runs/`, not `scribular-tauri-recording-p1/runs/`. |

## Changes

Applied in clauffice `db27497`; the 339 tests pass:

- `supervisor._post_pending_package` now also calls `demo.apply_pr_line`.
- `session_tokens.py` reads the task's `codex_home` and scans ±1 day of folders.

Proposed only:

1. The supervisor records the cwf source mtime or git HEAD at start. On change it re-execs, or at least logs a warning event, so a stale supervisor cannot loop.
2. Strip `<!-- cwf:agent-reply -->` markers from package files before posting.
3. `cwf demo register` dedupes on path, and the uitest `--record` output resolves relative to the task worktree.

These fixes affect future runs only. Running supervisors and sessions keep the code they loaded.
