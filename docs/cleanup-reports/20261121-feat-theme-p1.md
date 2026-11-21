# Cleanup report: 20261121-feat-theme-p1

## Summary

- **Outcome: merged.** PR #49 (Theme PR1, appearance state, AS1–AS5) merged at `cc072c6c338a6dae5122c1fe97f39c813ccdf638` on 2026-11-21 11:33 UTC. Task head was `61afe4a`.
- Four build→verify→review rounds. Round 1 produced the PR; rounds 2–4 fixed Cubic and agent-review findings (6, then 2). No premature stops, compactions, resumes or false-ready reports.
- Only one non-review intervention: a usage-limit resume (infrastructure, 38 min stall).
- Biggest cost: verify sessions used 19.5M of 32.5M tokens (60%). Each verify re-ran every suite, rebuilt the debug bundle and re-recorded the same demo for every head.
- Weak spots: review package misses "Proof of success", the mermaid journey and the "Human Verification Needed" title; a startup panic in the retained test profile was flagged and never investigated.
- Next part: PR2 (see end of report).

## Metrics

| Metric | Value |
|---|---|
| Terminal state | cleanup (PR merged) |
| Active time | 6552 s (109 min) |
| Waiting on human | 13956 s (233 min) |
| Waiting on CI | 1138 s (19 min) |
| Review rounds (cwf) | 0 |
| Fix pushes | 4 |
| Compactions / resumes | 0 / 0 |
| Interventions logged | 6 (1 real, 4 agent-review dispatches, 1 cleanup prompt) |
| Premature stops / stop blocks / false-ready | 0 / 0 / 0 |
| Demos | 1 (happy-path), 0 stale |
| Unverified manual items | 0 |
| Tokens (estimated for Codex) | 32.45M total: build 10.66M, verify 19.51M, review 2.15M, cleanup 0.14M |

Models: build, verify and review ran on harness codex, model `gpt-6-sol` (events record `model: null`; the name comes from `session_tokens.py`). Cleanup ran on claude `sonnet`.

## Process review

**Value per agent session.** 13 sessions: 4 build, 4 verify, 4 review, 1 cleanup.
- Build sessions were short (3–5 min) apart from the first (47 min, of which 38 min was the usage-limit stall).
- Verify re-ran the same three suites plus a debug build, a uitest and a recording on every head (events 06:30, 09:07, 09:35, 09:51). The last head, `61afe4a`, changed one function (`resetMyTheme`) and one test, yet cost 5.5M tokens and 9.7 min. `cwf delta` marked suites valid, but the native uitest and demo were redone anyway.
- Review sessions were 1–2 min each and each reviewed a distinct head (no head reviewed twice). Cubic caught 6 findings on `46ab2ee` and 2 on `d79d17a`, so it paid for itself.
- Cubic was added as a reviewer at 08:51, 135 min after `product_review_ready` (06:35). Round 1 had been approved without it, then reopened. Evidence: transition at 08:51:27 "Cubic is now a configured reviewer".

**Communication.** `<task dir>/handoffs/` does not exist, so the skill's handoff-vs-next-session comparison is unknown. Only `reports/*.ingested.json` and transition reasons exist, and I compared those instead.
- Dropped: verify at 09:35 wrote "Retained test profile startup panics before license bypass; review this environment/profile risk". The review session did not investigate it, and the merged package still lists it as an open "Concern".
- Repeated rediscovery: every verify session re-found that the plain debug build fails on the placeholder updater key and that `tools/uitest/tauri-debug-overlay.json` fixes it (events 06:30, 09:07, 09:51). Also, root `cargo test --lib` fails and must run from `src-tauri` (09:51). Neither is in the project docs.
- Dropped: build at 09:17 noted repo-wide `cargo fmt --check` fails on `commands.rs`, `license/mod.rs`, `update/mod.rs`. Nobody filed it.

**Time.**
- Build 05:36–06:23 (47 min, includes the 05:40–06:18 usage limit); verify 8 min; review 2 min.
- Round 2: build 3 min, verify 13 min, review 2 min. Round 3: build 5 min, verify 18 min, review 1 min. Round 4: build 3 min, verify 10 min, review 2 min.
- Idle gaps: 136 min after the first product-review-ready (waiting on Cubic config/human), 100 min from the last review to merge, and 0–3 min between all phases. Handoffs were smooth; no stalls other than the usage limit.
- Environment gates: usage limit (`session_usage_limit` 05:39:59, reset 22:17 PST, resumed 06:18 UTC): `infrastructure`. License/trial lock: `operator_note` was already set, so verify did not stall. Retained test profile panic: `infrastructure`, cause unknown.

**Tokens.** From `session_tokens.py`; Codex counts are estimates, and all 13 sessions have usage data. Verify 60%, build 33%, review 7%.

**Parallel implementation.** The spec split work across 3 PRs, not across agents. One build agent per phase; nothing to check.

**PR hygiene.** One product-review comment (id 7058702561), edited in place per head, no duplicates. Four agent-review comments, one per head, none repeated. The PR body starts with a Summary, not a recording name, and the cubic summary tracks the latest head. The evidence files cited in the package live under `/tmp/scribular-theme-p1-verify/` (not durable); the recording is in the task `recordings/` directory.

**Demo evidence.** One happy-path recording, 58.9 s, rewritten under the same name on each head (4 `demo_added` events). It shows Welcome → Settings → Appearance. PR1 has no Appearance controls, so the feature's surface (state, persistence, preset resolution) is not visible in it. The test scenario passed, but the video only shows navigation. The filename does not follow `NN-slug-happy-path`. Dead lead-in length is unknown: I did not inspect frames.

**Product review package.** Posted comment misses all three template requirements:
- The block is titled "Recording", not "Proof of success", and has no mermaid journey diagram.
- "Test it yourself" is not titled "Human Verification Needed" (it says "None.", which is correct content).
- The recording line is a local absolute path, not viewable from the PR.

## Classified items

| # | Item | Class | Evidence |
|---|---|---|---|
| 1 | Usage-limit resume, build session, 38 min | infrastructure | events line 6 `session_usage_limit`; `logs/prompt-0001.txt` |
| 2 | Agent-review dispatch ×4 (prompt-0002..0005) | task-notification, filtered | `logs/prompt-0002..0005.txt`; not interventions |
| 3 | Cleanup prompt (prompt-0006) | task-notification, filtered | `logs/prompt-0006.txt` |
| 4 | Round 1 approved before Cubic was configured, reopened with 6 findings | infrastructure | transition 08:51:27 |
| 5 | Plain debug build fails on placeholder updater key; overlay rediscovered per verify | missing_capability | verify transitions 06:30, 09:07, 09:51 |
| 6 | Verify re-records demo and re-runs native uitest per head | bad_instruction | 4 `demo_added` events; verify 09:42 changed one function |
| 7 | Retained-profile startup panic flagged, not investigated | product_uncertainty (unknown cause) | verify 09:35; package "Concern" |
| 8 | Package misses "Proof of success", mermaid and "Human Verification Needed" | bad_instruction | comment 7058702561 |
| 9 | Demo shows navigation only for a state-only PR | product_uncertainty | recording; spec PR1 scope |

## Proposed changes

Nothing was applied (automatic mode, step 8 skipped). Applies to future runs only.

1. **Target:** `workflows/coding/skills/coding-task/templates/review-package.md`. **Change:** title the recording block "Proof of success" and add a required mermaid `flowchart` of the demo's user journey; rename "Test it yourself" to "Human Verification Needed" with numbered one-action steps and "None." when empty; require a PR-viewable recording link or attachment, not a local path. **Evidence:** item 8.
2. **Target:** `workflows/coding/skills/verify/SKILL.md` (verify phase). **Change:** when `cwf delta` shows only test-file and one-function changes, rerun only the affected suites and reuse the previous demo; re-record only if the demo's visible steps changed. State-only PRs may skip the demo and say so. **Evidence:** items 6, 9; verify 09:42 used 5.5M tokens for a one-function fix.
3. **Target:** `docs/` or `tools/uitest/README.md` in the product repo. **Change:** add "Debug bundle for uitest on this Mac": the `tools/uitest/tauri-debug-overlay.json` overlay because the updater public key is a placeholder, `SCRIBULAR_SKIP_LICENSE=1`, and `cargo test --lib` runs from `src-tauri`. **Evidence:** item 5; verify 09:51.
4. **Target:** `watcher-config.json` template/`operator_note` (cwf). **Change:** add the overlay and `src-tauri` cargo note to the operator note for Scribular tasks. **Evidence:** item 5.
5. **Target:** `workflows/coding/skills/review/SKILL.md`. **Change:** when a verify handoff lists an environment/profile risk, the review session must reproduce it or file an issue, not forward it as a "Concern". **Evidence:** item 7.
6. **Target:** cwf reviewer setup (`cwf init` defaults). **Change:** configure Cubic at task creation so the first `product_review_ready` already includes Cubic findings. **Evidence:** item 4 (136 min, 3 extra rounds).
7. **Target:** `workflows/coding/skills/cleanup` and cwf. **Change:** write `handoffs/*.md` into the task dir, or drop the comparison from step 4 if handoffs are not kept. **Evidence:** `handoffs/` absent.
8. **Target:** product repo issue tracker. **Change:** file an issue for pre-existing `cargo fmt --check` failures in `commands.rs`, `license/mod.rs`, `update/mod.rs`, and for the retained-profile startup panic. **Evidence:** build 09:17, verify 09:35.

## Next part

Spec has PR2 (picture import/storage/cleanup, bundled assets, scoped CSP/asset access, Stars behavior; AS6–AS8, AS11). Surviving spec copy: `$HOME/.local/state/clauffice/workflows/pi0neerpat__scribular-tauri/20261121-feat-theme-p1/2026-11-16-theme-spec.md`.

Branch: `feat/theme-p2`

```
cwf init --repo-dir "$HOME/Projects/Scribular/scribular-tauri-theme-p2" --spec "$HOME/.local/state/clauffice/workflows/pi0neerpat__scribular-tauri/20261121-feat-theme-p1/2026-11-16-theme-spec.md" --part 2
```
