# Cleanup report: 20261119-feat-remote-llm-uitest (PR #35)

## Summary

- **Outcome:** merged. PR #35 `feat(llm): remote-llm feature with llama-server summarizer for VM uitests`, merge commit `b8f73c73f5bbfbf9aace1bcdc9beb209250fc13e` (2026-11-19 06:54:52Z). Final head `1f23f26`.
- **Shipped:** opt-in `remote-llm` Cargo feature (`RemoteSummarizer`, `src-tauri/src/llm/remote.rs`), uitest forwarding of `SCRIBULAR_LLM_URL`, README/CLAUDE.md host-server docs.
- **Not proven:** the point of the feature (a real summary served by host llama-server, AS1) was never observed in the real app. Patrick waived the demo; AS1 stayed under "Needs Patrick".
- **Process:** 10 sessions, ~39 min of agent wall time, ~6.28M tokens (almost all cache reads), 86 min task start to merge. 0 premature stops, 0 false-ready, 0 compactions, 0 review rounds counted by cwf (Cubic blocking threads: 8 on the first head, 1 on the second, 0 on the third).
- **Main avoidable cost:** one verify session (24.75 min, 2.18M tokens, 7 uitest runs) went looking for a way to trigger a summary that did not exist; and a docs-only one-line fix took a full review→build→verify→review loop (~0.88M tokens).
- **Main human cost:** one `needs_human` block (≈7.6 min) because the product gate needs a demo or waiver, and the skill does not say a recorded harness gap needs a waiver.

## Metrics

| Metric | Value |
| --- | --- |
| Terminal state | cleanup (PR merged) |
| Active / human-wait / CI-wait | 2255.8 s (37.6 min) / 1910.8 s (31.8 min) / 1092.7 s (18.2 min) |
| Sessions | 10 (3 build opus, 3 verify sonnet, 4 review sonnet) |
| Review rounds / fix pushes | 0 / 3 |
| Compactions / resumes | 0 / 0 (one human `resume` for the waiver, event 102) |
| Premature stops / stop blocks / superseded | 0 / 0 / 0 |
| False-ready | 0 |
| Unverified manual items | 1 (AS1) |
| Demos / stale demos | 0 / 0 (waived, event 108) |
| Screenshots | 0 |
| Reviewer network errors | 7 (events 26,28,30,32,34,89,91) |

Models are the `model` field of `session_start`: build `opus`, verify and review `sonnet`. Exact model IDs are not in the events; unknown.

## Process review

**Value per agent session**

| # | Phase (model) | Wall | Tokens | Verdict |
| --- | --- | --- | --- | --- |
| 1 | build (opus) | 4.9 min | 1.30M | Valuable. Whole feature + 10 unit tests + swift change in one session; handoff (event 9) lists commands and counts. |
| 2 | verify (sonnet) | 24.8 min | 2.18M | Mixed. Confirmed launch log shows the remote URL (event 18). Then spent most of the time on AS1: `system-audio-smoke` failed because Settings no longer shows "App audio" (`evidence/uitest2.log`: `click "App audio" index 1: not found`), and Regenerate on the sample session failed because "Sample sessions can't be regenerated" (`evidence/uitest6.log`, screen text) so no request ever reached the server. 7 runs in `runs/`. Ended with an honest "NOT verified" handoff (event 21). |
| 3 | review (sonnet) | 0.5 min | 0.19M | Router only: read 8 Cubic threads, wrote the fix list (event 40). Useful, cheap. |
| 4 | build (opus) | 2.6 min | 0.85M | Fixed all 8 threads in one commit `3f2e1e6`. |
| 5 | verify (sonnet) | 1.6 min | 0.34M | Re-ran cargo tests, pushed. |
| 6 | review (sonnet) | 1.1 min | 0.26M | Router: 1 open P2 (`-c 32768`). |
| 7 | build (opus) | 0.5 min | 0.34M | Three-file docs edit. |
| 8 | verify (sonnet) | 1.0 min | 0.29M | Ran `git diff` + `uitest doctor`, pushed, resolved thread. |
| 9 | review (sonnet) | 1.1 min | 0.19M | Cubic approved; `cwf gate product` failed on missing demo/waiver → `needs_human` (event 99). Same head `1f23f26` that session 8 already said "needs waiver or Patrick" about (event 84). |
| 10 | review (sonnet) | 1.1 min | 0.35M | After waiver: waived, posted package. Necessary. |

Overlap: sessions 6→8 (review→build→verify) cost ~0.88M tokens and ~2.7 min of sessions plus ~3 min waiting for Cubic for a docs-only change to a flag in three files. Sessions 9 and 10 reviewed the same head; the first could not have finished without Patrick, so it was necessary, but the block could have been raised by session 8 (it already knew, event 84).

**Communication (handoff vs next session)**

- Event 21 (verify→review) asked review to "review remote.rs diff; Patrick decides to accept or run a manual Regenerate". Review (session 3) instead handled Cubic threads; the diff review is Cubic's job here, so nothing was lost, but the handoff's first action was not followed.
- Event 47 (build) said `clippy -D warnings` and `cargo fmt --check` fail on pre-existing code outside `remote.rs`. No later handoff, package or PR text mentions it. Dropped (low impact, but unrecorded).
- Plan task 3 step 3 (`as11-note-style-default` with `SCRIBULAR_LLM_URL` on the VM, expecting `/completion` requests in the host log) has no verification record and no mention in any handoff or the package. Dropped without a stated reason.
- PR body says "cargo test … pass (recorded on this head)" but the cargo tests were recorded at `64bdd42`/`3f2e1e6`, and head is `1f23f26` (verification entries marked stale in `task.json`). The package comment says "earlier head" correctly; the body is inaccurate.
- Event 108's waiver reason and the AS1 text carried through correctly to the package.

**Time**

- Session wall times in the table above; total ≈39.2 min. Longest: verify #1 (24.8 min).
- Idle gaps (from `session_tokens.py`): verify→review 6.1, 5.1, 2.8 min (Cubic running, with network-error polls); review→review 8.0 min (human waiver).
- After `product_review_ready` (06:31) to merge (06:55): 24 min of human review.
- Smoothest handoffs: build→verify (0.0–0.01 min gaps) and review→build.

**Tokens** (`session_tokens.py`): total 6,280,854; build 2,488,375; verify 2,812,201; review 980,278. Input is almost entirely cache reads (5.81M of 6.28M); uncached input is 212 tokens. Codex: not used. Compactions: 0.

**Parallel implementation:** the plan header recommends `superpowers:subagent-driven-development`. No session, event or tool-call record shows a split: build ran as one opus session (tool-call count 5 per the script; how it counts is not verified). Whether subagents ran inside it is unknown.

**PR hygiene**

- One package comment (id 7001668646); no duplicates or stale packages. No recording filename in the description: the top line is `Recording: none - harness gap …`. The body line still says "harness gap" while the package says "demo waived"; consistent enough.
- Cubic's two review summaries both say "All reported issues were addressed". Cubic warned the monthly reviewed-line limit was at ~91% then ~93% (quota risk for later PRs).
- Evidence files: `runs/` (16 MB, ignored by git) holds the 7 uitest runs, copied to `<task dir>/evidence/runs`; none is cited in the PR.

**Demo evidence:** none recorded; waived by Patrick for a harness gap, correctly not recorded as a fake happy path. The feature's surface could not be captured: summaries only run after a real recording, and uitest had no working recording path.

## Classified items

| Item | Evidence | Class | Note |
| --- | --- | --- | --- |
| Supervisor phase prompts 0001, 0002, 0004–0010 | `logs/prompt-*.txt` | not interventions | Standard phase dispatch; no human text. |
| prompt-0003 task-notification | `logs/prompt-0003.txt` | filtered | A background `llama-server` + build command reported "failed exit 7" (the trailing health `curl`). |
| prompt-0011 operator waiver note | event 106; "Patrick approves a demo waiver" | product_uncertainty | Waiving a demo is a human gate; keep. |
| Block `needs_human` for missing demo/waiver | events 99–101, 102 | bad_instruction | Verify #2 already recorded the harness gap (event 21, 84); SKILL.md lines 107–108 ("needs a recorded demo or a waiver") and 139 are silent on how a harness gap gets a waiver, so the block surfaced only after Cubic approved. |
| AS1 unreachable by uitest | `evidence/uitest2.log`, `uitest6.log` | missing_capability | No scenario can trigger a summary; `system-audio-smoke` is stale (no "App audio" in Settings) and samples can't Regenerate. Project `CLAUDE.md` still says to start from `system-audio-smoke.uitest`. Side-quest per skill line 139. |
| Host llama-server died during verify | `evidence/llama.log`: listening on `127.0.0.1:5801`, then "cleaning up before exit" 8 s later | model_error | Launched with `nohup` inside a background shell task (prompt-0003), which ended and killed it; the later verify handoff blames "harness/UI drift" only. Whether a later run had a live server is unknown (single log). |
| Plan task 3 step 3 (`as11`) not run/recorded | no record in events, handoffs or package | model_error | Instruction was in the spec; no evidence it was attempted. |
| Pre-existing clippy/fmt failure not carried | event 47 vs later handoffs | model_error | Minor. |
| 7 `reviewer_network_error` events | events 26–34, 89, 91 | infrastructure | Poll retried and recovered each time. |
| Cubic quota 91%→93% | Cubic review summaries | infrastructure | Not a process fault. |
| Docs-only fix looped through 3 sessions | sessions 6–8 | bad_instruction (mild) | Skill has no fast path for a docs-only review fix; a docs-only change could be pushed by the build or review session itself with `cwf reconcile`. Do not skip Cubic re-review. |

## Proposed changes (all future runs only; installed skills pick these up in new sessions)

1. **`workflows/coding/skills/coding-task/SKILL.md` ~line 139** (harness gap): add "…and say in the same handoff that the product gate needs a demo waiver (`cwf demo waive --reason …`) for Patrick, so `needs_human` is raised by verify, not discovered after Cubic approves."
2. **Same file, background services**: "Start host servers a verify run depends on with `tmux`/`setsid` or the harness `run_in_background` with a `kill -0` check and a `/health` poll before the first uitest; a server started inside a task shell dies with the task."
3. **Same file, handoff**: "Carry 'pre-existing lint/format failures' and any spec step you did not run (with the reason) into the handoff Open items."
4. **Project docs (Scribular, not shared skills):** fix `CLAUDE.md` E2E bullet that says to start from `system-audio-smoke.uitest` (stale: no "App audio" control); open a uitest side-quest for a summary-triggering scenario (record a short session without the mic path).
5. **Design question for Patrick:** a docs-only review fix could skip the verify session. Not applied; it touches review/verify gating.

Applied by this cleanup: nothing in the clauffice checkout (proposals 1–3 have a single evidence instance each, below the "small, evidence-backed" bar; see `git -C ~/Projects/clauffice` unchanged).
