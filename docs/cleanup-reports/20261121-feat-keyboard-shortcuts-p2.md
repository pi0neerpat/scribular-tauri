# Cleanup report: 20261121-feat-keyboard-shortcuts-p2

**Outcome: merged.** PR #52 merged 2026-11-23T18:20:49Z, merge commit `73d8c19`. Final head `dff38b8` (Cubic approved). Spec: `plans/2026-11-16-keyboard-shortcuts-spec.md` part 2 (last part of the P2 split; no next part named).

**Summary.** The feature shipped, but the run took 2.5 days wall clock and about 137M tokens (known sessions). Cause: the codex harness hit 10 usage limits and 9 verify sessions timed out idle. 9 fix pushes. The final head differs from the product-reviewed `1e4e67d` (false ready). AS7a is unverified (pre-existing `pick-call-app` timeout), AS7b is manual-only. The PR body still says AS7a passed all 79 steps, which contradicts the handoff. Patrick should treat the PR body as stale.

## Metrics

| Metric | Value |
|---|---|
| Active / human-wait / CI-wait | 202,732 s (56.3 h) / 4,984 s / 15,012 s |
| Review rounds / fix pushes | 0 / 9 |
| Compactions / resumes / premature stops / stop blocks | 0 / 0 / 0 / 0 |
| Interventions | 3 (2 usage-limit resumes, 1 cleanup prompt) |
| False ready | 1 (`1e4e67d`, head changed while presented) |
| Sessions | 37 start, 35 end; 2 ended without handoff (codex verify at limit) |
| Models | codex gpt-6.2-sol (build/verify/review until 2026-11-23 17:46); claude sonnet x5 + opus x1 after |
| Tokens (known 29/37) | total 137.1M: verify 89.7M, build 43.2M, review 4.1M, other 0.2M |
| Demos | 1 happy-path, 0 stale |
| Unverified manual | AS7 (AS7a + AS7b) |

## Classified items

| Item | Class | Evidence |
|---|---|---|
| Intervention 1 (`logs/prompt-0001.txt`) "Usage limit reset" | infrastructure | `session_usage_limit` 2026-11-22T05:56 |
| Intervention 2 (`logs/prompt-0002.txt`) "Usage limit reset" | infrastructure | `session_usage_limit` 2026-11-22T13:17 (reset Nov 27 12:16) |
| Intervention 3 (`logs/prompt-0003.txt`) cleanup prompt | not an intervention (task notification/cleanup trigger) | filtered |
| False ready `1e4e67d` | missing_capability | product_review_ready 2026-11-22T04:46, head changed 05:55 to `72766f3` (PR-feedback fixes) |
| 17:47 blocker: uitest doctor reports tmux MISSING Accessibility/Screen Recording/Automation | infrastructure | `needs_human` 2026-11-23T17:47; supervisor started outside `cwf-watch` tmux (see CLAUDE.md) |
| 6 "verify session idle beyond timeout" needs_human | infrastructure | events 11-21T11:23, 14:34, 11-22T03:40, 11-23T06:37, 11:45, 16:47; all follow a codex usage limit |
| AS7a `pick-call-app` "Choose an app" not found | infrastructure (pre-existing AudioCapCLI source-list timeout) | handoff 2026-11-23T18:16; log `/private/tmp/claude-501/.../32a5033a-.../scratchpad/as7.log` |
| PR body claims AS7a passed 79 steps | model_error | PR body vs handoff `unverified_manual` |

No premature stops or stop blocks.

## Process review

- **Value per session.** The 36 build/verify/review sessions reduce to a handful of real changes (head changes: 9). Many build sessions lasted 1 to 8 min with 0 to 4 tool calls (e.g. build 18:07 opus, 1.09 min, 2 tools; build 15:06 codex, 0 tools). Those are no-op build phases between verify and review. Verify repeated heavy work: 9:54 to 14:36 verify used 20.9M tokens over 282 min, then three more verify rounds on `c73f3fd`/`4f2ba68` (14:48, 15:13, 15:52) re-ran the same suites. Same-head reviews: `3545329` and `4f2ba68` reviewed once each; `c73f3fd` had verify x3 on one head.
- **Communication.** Handoff files are under `reports/` (no `handoffs/` dir). The operator note's scope items (drop `6a47a99`, AS7a only, pick-call-app out of scope) were followed in the claude sessions. Dropped item: the PR description was not refreshed after AS7a became unverified (kept "passed all 79 native steps").
- **Time.** Longest gaps: 70.5 min review to build (usage-limit wait), 80 min verify (16:26 to 17:46, codex session with 0 tokens: limit-blocked), 10.9 min verify to verify. Environment gates that stalled verify: (1) codex usage limit x10; (2) tmux missing Accessibility/Screen Recording at 17:47 (class infrastructure). Proposal: add `operator_note` in `watcher-config.json` "start supervisor only from cwf-watch tmux; run `uitest doctor` before verify" and a project-doc note that the license gate uses `SCRIBULAR_SKIP_LICENSE=1` on a debug build (already approved).
- **Tokens.** See metrics. 8 of 37 sessions have unknown usage (not estimated). Claude sessions after the harness switch cost about 0.2 to 1.0M each versus 2 to 20M for codex verify.
- **Parallel implementation.** The spec did not recommend splitting; one agent per phase.
- **PR hygiene.** One comment (the product review package, `<!-- cwf:agent-reply -->`); no duplicates. PR body lists a recording by absolute task-state path at the line "Recording:" mid-description (not the top). Stale claim noted above.
- **Demo evidence.** One happy-path demo (250 ms pace, 2026-11-23 file); 12 recordings exist in `recordings/` incl. earlier superseded ones. Dead lead-in length was not measured here (unknown). Recording naming varies (`-817`, `-final`, spec-slug); only the spec-slug ones came from `cwf demo`.
- **Product review package.** Not re-read in this session; `review-package-1e4e67d.md` is for the superseded head. Unknown whether it has "Proof of success" + mermaid; check with `cwf package post` output.

## Proposed changes

1. `workflows/coding/skills/verify/SKILL.md`: when a verify session ends with a recorded unverified scenario, require updating the PR "Verification" section (and removing "passed" claims) before `product_review_ready`. Evidence: PR #52 body vs handoff 2026-11-23T18:16.
2. `workflows/coding/` supervisor: skip build phase when verify handoff has no outstanding findings (build sessions of 1 to 8 min, 0 to 4 tools at 15:06, 18:07). Evidence: session token table above.
3. `workflows/coding/` watcher: when a codex session is limit-blocked, switch harness automatically (auto switch happened only by operator at 17:46, ~80 min after the limit at 16:26). Evidence: sessions 16:26 to 17:46, 0 tokens.
4. `watcher-config.json` `operator_note` for this repo: start supervisor only in tmux `cwf-watch`; run `uitest doctor` first. Evidence: `needs_human` 2026-11-23T17:47.
5. Product repo follow-up (separate task): fix AudioCapCLI source-list timeout in `pick-call-app` so `tools/uitest/scenarios/as7-background-shortcut.uitest` runs; then re-run AS7a.

## Human Verification Needed

1. AS7b (manual-only): start a third-party app that claims the proposed chord.
2. In Scribular > Settings > Keyboard Shortcuts, try the same chord.
3. Record macOS version, competing app, active chord, and whether macOS rejects or shares it.
4. AS7a: after follow-up 5, run `swift run --package-path tools/uitest uitest tools/uitest/scenarios/as7-background-shortcut.uitest --app <debug Scribular.app>` with `SCRIBULAR_SKIP_LICENSE=1`.
