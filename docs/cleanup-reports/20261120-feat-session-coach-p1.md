# Cleanup report: 20261120-feat-session-coach-p1

## Summary

PR #42 (PR1 of the session Coach: local queued job, cancellation, grounding checks, typed IPC) merged as `dbfe9d0488c5b01ef72d8a6ff9d18d685c05b70f`. Reviewer (Cubic/agent) approved head `f9194a7`.
The run took ~5.4 h active, 2.8 h waiting on a human and 16 min waiting on CI. Five fix pushes. Most of the wall time (231 min verify session, 21:31 to 01:23) was spent on one environment gate: the licence/keychain prompt blocking native uitest.
The Patrick-requested feature-visible happy-path re-record was **never achieved**. The registered demo (`recordings/2026-11-20-...-p1-happy-path.mp4`, 44.8 s, head f9194a7) is the one flagged as showing only the licence block; the PR merged with it. PR1 has no UI caller, so the feature has no visible surface in the app (see Demo evidence).
This cleanup session did not re-record: the PR is merged, and the only unblock was a keychain prompt that needs Patrick's click. Not executed here.

## Metrics

| Metric | Value |
|---|---|
| Terminal state | merged -> cleanup |
| Active / human-wait / CI-wait | 19271 s / 10008 s / 938 s |
| Review rounds (cwf counter) | 0 (Cubic rounds were actually 3: 11, 3-5, 2 threads) |
| Fix pushes | 5 |
| Compactions / resumes / premature stops / stop blocks / false ready | 0 / 0 / 0 / 0 / 0 |
| Interventions | 3 (2 "Usage limit reset", 1 cleanup prompt) |
| Unverified manual | 4 (AS2, AS6, AS10, AS11) |
| Demos | 1 happy-path, not stale, but invalid (licence block) |

Sessions: build/verify/review phases on codex `gpt-6.2-sol` (model id not in events: `model: null`); cleanup on claude `sonnet`.

## Process review

**Value per session.** 18 sessions. Build->verify->build ping-pong early: verify on a26d836 (3 min) only reported that no native scenario can exercise PR1 IPC and sent work back to build; that conclusion was knowable from the spec (PR1 has no UI). Verify sessions at 19:53, 20:20, 20:32, 20:49 each re-ran the same npm/cargo suites and the same two native smokes (`session-coach-p1-native-smoke`, `-screenshot`) on every head: 4x repeated. Three Cubic rounds each produced valid findings, so review was beneficial; however thread 4647095316 was re-flagged after being answered, and two threads (P1 cancel routing, P2 terminator) were reported on a head that already contained the fix (events 20:57:30).

**Communication.** Handoffs repeatedly carried the same open items forward: native IPC unobserved (AS2/AS6) appears in 5 consecutive handoffs without a decision. The 20:28 handoff said Cubic had posted 3 blocking threads; the 20:31 build addressed only one with new code (others "already covered"), which was correct but only confirmed two sessions later. The 21:19 and 21:28 handoffs said "do not retry this gate"; the operator note (debug build + `SCRIBULAR_SKIP_LICENSE=1`) arrived only after two blocked cycles. Dropped: the happy-path re-record instruction (never fulfilled); the 01:23 handoff's own note "a feature-visible demo needs a scope or evidence decision" was never answered before merge.

**Time.** Stalls: 21:15-21:19 and 21:22-21:28 (release build ignores `SCRIBULAR_SKIP_LICENSE`), 21:31 -> 01:23 (231 min: SecurityAgent keychain prompt on the debug app waiting for a click), then 153.9 min idle (01:23 -> 03:56) until operator resume at 03:49. Smooth handoffs: build->verify within 0.1 s each time; reviewer-to-build 0.01 min.

**Tokens.** `session_token.py`: known only for 2 of 18 sessions (build session 1: 313k tokens, cleanup: 159k). All other codex sessions: unknown (no transcript usage found). Grand known total 471,624; not an estimate of the whole task.

**Parallel implementation.** Spec suggests splitting into PR1/PR2/PR3 (spec lines 125-129), which was followed (this is PR1 only). It did not recommend splitting PR1 across agents. N/A.

**PR hygiene.** One review-package comment on PR #42, no duplicates. PR body opens with a "Recording:" line holding the absolute path of the mp4 at the top of the description (flag: recording filename at top). Body says the screenshot "shows the app at the welcome overlay". Evidence files exist (`/tmp/*.log`, `runs/` ignored dir, recording in task dir). The package is not stale (`package_head` = head).

**Demo evidence.** Recording was named by spec and registered with `--kind happy-path`, but the feature cannot be captured: PR1 has typed IPC and no UI (`session_coach_start` has no caller until PR2). The only visible surface is Note Styles / welcome overlay. The merged demo therefore shows a licence block / welcome screen, not the feature (flagged by Patrick at 21:15). Dead lead-in: not measured (video not decoded). Recommend a spec-level rule: a backend-only PR gets no happy-path demo; it gets an AS evidence log instead.

**Product review package.** The posted package uses a "Test it yourself" heading, not "Human Verification Needed"; it has no "Proof of success" block and no mermaid user-journey diagram. Proposed fix below.

## Classified items

| # | Item | Class | Evidence |
|---|---|---|---|
| 1 | Intervention 20:15:56 "Usage limit reset" | infrastructure | logs/prompt-0001.txt |
| 2 | Intervention 01:17:05 "Usage limit reset" | infrastructure | logs/prompt-0002.txt |
| 3 | Intervention 03:57:01 cleanup prompt | not an intervention (supervisor-issued phase prompt; filtered) | logs/prompt-0003.txt |
| 4 | Release build shows "Trial ended", ignores SCRIBULAR_SKIP_LICENSE (21:19, 21:28) | infrastructure | events 21:19:09, 21:28:18; /tmp/session-coach-p1-fresh-launch*.log |
| 5 | Keychain SecurityAgent prompt blocked debug app (231 min) | infrastructure | event 01:23:04; /tmp/session-coach-p1-debug-screen.png |
| 6 | Demo shows licence block, feature has no UI | product_uncertainty | event 21:15:14; handoff note 01:23 |
| 7 | Verify repeated unchanged native smokes on each head | bad_instruction | verification list: smoke/screenshot 4 times |
| 8 | `cargo test --lib` run from repo root failed 6 times (Cargo.toml is in `src-tauri`) | bad_instruction | verification entries `cargo test --lib` fail (20:22, 20:33, 20:50, 21:16, 21:23, 21:32) |
| 9 | `tauri build --bundles app` exits 1 without `TAURI_SIGNING_PRIVATE_KEY` | bad_instruction | verification entries 20:22, 20:34, 20:51 (CLAUDE.md now documents the key; the first runs predate it) |
| 10 | Review package heading/mermaid missing | bad_instruction | PR #42 comment |

## Proposed changes (not applied; automatic mode, step 8 skipped)

1. **File:** `workflows/coding/skills/verify/SKILL.md`. **Change:** when the diff has no UI caller for the feature (backend-only PR), do not record a happy-path demo; record AS evidence logs and mark the demo "n/a: no visible surface". **Evidence:** item 6, handoff 01:23.
2. **File:** `workflows/coding/skills/verify/SKILL.md`. **Change:** carry native smoke/screenshot results when the delta touches no frontend or bundled assets; re-run only affected scenarios. **Evidence:** item 7.
3. **File:** project `CLAUDE.md` (scribular-tauri). **Change:** note `cargo` commands run from `src-tauri`; `cargo test --lib` at repo root always fails. **Evidence:** item 8 (6 failures).
4. **File:** `workflows/coding/coding-task/templates/review-package.md`. **Change:** rename "Test it yourself" to "Human Verification Needed" (numbered one-action steps, only human-only items), add a "Proof of success" block with a mermaid journey diagram. **Evidence:** item 10.
5. **File:** `watcher-config.json` for scribular tasks (`operator_note`) and project docs. **Change:** record that release builds ignore `SCRIBULAR_SKIP_LICENSE` and that uitest must use the debug bundle; run `bash tools/uitest/trust-build.sh --store-password` once before verify. **Evidence:** items 4-5 (about 4.5 h blocked).
6. **File:** `workflows/coding/skills/coding-task/SKILL.md`. **Change:** when the same unverified item appears in 3 consecutive handoffs, escalate it as a `product_uncertainty` question instead of carrying it. **Evidence:** AS2/AS6 carried 5 times.
7. **File:** PR description template. **Change:** do not put the recording absolute path on the first line. **Evidence:** PR hygiene.

## Open for Patrick

- Recording not redone; 4 unverified manual items: AS2, AS6 (native IPC, PR1 has no UI), AS10 (PR2), AS11 (PR3).

## Next part

Spec part 1 of 3; next is PR2 (Note-view conversation, comparison, Stop, guarded acceptance; AS1, AS3-AS6, AS10).
Branch: `feat/session-coach-p2`

```
cwf init --repo-dir "$HOME/Projects/Scribular/scribular-tauri-session-coach-p2" --spec "$HOME/.local/state/clauffice/workflows/pi0neerpat__scribular-tauri/20261120-feat-session-coach-p1/2026-11-16-session-coach-spec.md" --part 2
```
