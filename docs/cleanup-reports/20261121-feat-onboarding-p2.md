# Cleanup report: 20261121-feat-onboarding-p2

**Outcome: merged.** PR #50 (`feat/onboarding-p2`, onboarding PR2: first On a call explainer, app-audio permission status, retry path, persisted flag) merged as `cc4c7554c022e17c767cd28b8c7dca53f1745cbd` on 2026-11-23 05:17Z. Reviewed head `6354c6e`. AS5 and AS8 verified by agent; AS4 stayed manual-only and was verified by Patrick on 720cf89 (found a real bug, see below). One unverified manual item remains: AS4 on the final head (`unverified_manual_count: 1`). This is not a cwf self-task, so there is no `deployed_commit`.

Headline findings:
1. Wall time was dominated by waiting (usage-limit resets and overnight human wait), not work: 3 of 5 interventions were "Usage limit reset" nudges.
2. Patrick's one product intervention (sheet auto-opened System Settings) was an agent misread of R6; Cubic then pushed the behaviour back to "Settings on first explainer" (click only). Two rounds of build and verify went to that flip-flop.
3. One false `blocked: unexpected push`: verify's `head_at_start` was stale after the build phase committed 6354c6e.
4. The plain debug build fails on the updater pubkey placeholder on `origin/main` in every verify session; each re-discovered it and used a `/tmp` overlay.

## Metrics

| Metric | Value |
|---|---|
| Terminal state | cleanup (merged) |
| Active / human-wait / CI-wait | 127,910 s (35.5 h) / 27,022 s (7.5 h) / 20,835 s (5.8 h) |
| Review rounds (cwf counter) / fix pushes | 0 / 6 |
| Compactions / resumes | 0 / 0 |
| Interventions | 5 (4 real, 1 cleanup prompt filtered) |
| Premature stops / stop blocks / false-ready | 0 / 0 / 0 |
| Demos | 1 (happy-path, 3 recordings kept in `recordings/`), stale 0 |
| Models | build/verify/review: codex `gpt-6.2-sol` (event `model` field null; from token script); cleanup: claude `sonnet` |

Tokens (`session_tokens.py`; Codex counts estimated, known sessions only, 9 of 21): build 19.66M, verify 10.94M, review 1.05M, cleanup 0.14M, total 31.8M. Cached input is about 98%. 12 sessions have no transcript usage, so true totals are higher and unknown.

## Interventions

| # | When | Prompt | Class | Evidence |
|---|---|---|---|---|
| 1 | 11-21 21:39 | "Usage limit reset. Continue..." | infrastructure | `logs/prompt-0001.txt`; verify session 19:59-21:40 |
| 2 | 11-22 04:41 | Product feedback: sheet opens System Settings | model_error (spec text was explicit: anti-goal at spec:52, open question 1 at spec:119, so agent should not have called `openCallSettings()` unconditionally) | `product-feedback-1.md`; transition 11-22 04:41 |
| 3 | 11-22 08:14 | "Usage limit reset" | infrastructure | `logs/prompt-0003.txt`; verify session 08:42 ran 1175 min |
| 4 | 11-22 13:15 | "Usage limit reset" | infrastructure | `logs/prompt-0004.txt` |
| 5 | 11-23 05:18 | Cleanup prompt | filtered (workflow-issued, not an intervention) | `logs/prompt-0005.txt` |

Other classified item: `blocked: unexpected push` (11-23 04:17), the supervisor flagged 6354c6e as an unexpected push although it was the build phase's own commit. Class: missing_capability (verify's `head_at_start` was taken before the build commit was reconciled). Operator cleared it at 04:19 and restarted the session under codex-bravo. Evidence: transition reason text.

No premature stops or false-ready reports.

## Process review

**Value per session.** 21 sessions: 7 build, 7 verify (incl. 1 restart), 7 review. Useful: build 1 and 2 (initial feature, six Cubic fixes), verify rounds that found `app_attach_failed` once during the native call run. Overlapping: verify re-ran AS5 in-room start/stop and `npm test`/cargo suites on every head (4 times, 11/11 to 13/13 each) even when the delta only touched RecordView call-mode code; cwf "delta" carry rules cut some but not the native run. Review sessions 2 to 5 each took about 2 min and only dispatched Cubic threads, fine. Review at 11-21 14:10 ran 311 min with zero tokens: it was waiting (Cubic or usage limit); cannot tell which, transcript usage is missing.
Cubic churn: 6 threads (P1 granted-before-attach, P2 x3, P3), then 4, then 2, then 1, then 1 more at 3fff620 that reversed the feedback fix. Fix pushes 6 for one UI feature.

**Communication.** Handoffs live in transition `reason` text (no `handoffs/` dir exists). Dropped or missed: (a) build 11-22 04:41 read the feedback file but the earlier handoffs never carried the spec anti-goal (spec:52) into the build session, so R6 was misread in the first place; (b) the final transition says "Successful app-audio samples were not independently confirmed", i.e. app-audio capture working end to end was never shown by the demo, only meters; (c) the 11-22 08:40 review handoff correctly cited spec:119 and the build followed it. Smooth handoffs: build to verify throughout (0.0 to 0.14 min gaps).

**Time.** Longest sessions: verify 11-22 08:42 to 11-23 04:17 = 1175 min (usage limit plus operator switch to codex-bravo; infrastructure), verify 11-22 04:44 = 231 min, build 1 = 264 min, build 2 = 288 min, review 1 = 312 min. Largest idle gap: 416 min (11-21 21:45 to 11-22 04:41), waiting for Patrick's product review. Environment gates: (1) release-build license trial lock: infrastructure, handled by `operator_note` in `watcher-config.json` and used in every verify (no needs_human); (2) updater pubkey placeholder breaks the plain debug build: infrastructure/repo fault, every verify note repeats it (events 11-21 09:02, 14:04, 19:52, 21:40, 11-22 08:35). Proposal below. (3) Usage limit: 3 nudges.

**Tokens.** See metrics. Codex per-session usage unknown for 12 sessions (transcripts missing usage).

**Parallel implementation.** The spec's size note (PR2: M, 1-2 days) recommends no agent split. Done by single agent per phase. Nothing to flag.

**PR hygiene.** 1 comment on PR #50: the product review package for 6354c6e, posted once, no duplicates. PR body has cubic summary and a verification section with no recording filename at top. Evidence files cited in PR (`/tmp/scribular-onboarding-p2-*.log`) are in `/tmp` and may not survive; the three demo recordings are in the task `recordings/`.

**Demo evidence.** One happy-path demo (AS5 in-room and On a call), 22/22 steps on 6354c6e. The earlier demo runs (96e2eb2, 52c85cb, 720cf89, 3fff620) were re-recorded each head: three recordings kept. Dead lead-in and total lengths not measured (no ffprobe run; unknown). The demo shows meters, not captured app audio. It does not show an environment gate (license skipped via debug bundle).

**Product review package.** The package comment has "What this PR does" and "Test it yourself" with AS4. Per skill step 4 it should be "Human Verification Needed", needs a "Proof of success" block with a mermaid user-journey diagram, and numbered one-action steps. The posted body (first 600 chars viewed) uses "Test it yourself". Proposal 3 below.

## Proposed changes

| # | Target file | Change | Evidence |
|---|---|---|---|
| 1 | `workflows/coding/skills/coding-task/templates/review-package.md` | Rename "Test it yourself" to "Human Verification Needed", add "Proof of success" with mermaid journey diagram, numbered one-action steps | PR #50 package comment uses "Test it yourself" |
| 2 | scribular-tauri `tauri.conf.json` or `tools/uitest/README.md` | Document or fix the updater pubkey placeholder so `npm run tauri -- build --debug --bundles app` works without a `/tmp` overlay; add a one-line note to CLAUDE.md "E2E / uitest" | verify transitions 11-21 09:02, 14:04, 19:52, 21:40, 11-22 08:35 all report the failure |
| 3 | `workflows/coding/skills/coding-task/SKILL.md` (build phase) | When a spec has "anti-goals" or "open questions" tied to a requirement, the build handoff must quote them next to the requirement | product-feedback-1.md: agent misread R6 against spec:52 and :119 |
| 4 | cwf watcher (`workflows/coding/cwf` push check) | When a build commit is local and verify starts, refresh `head_at_start` after the implementing to verifying transition so the supervisor does not report "unexpected push" | event 11-23 04:17 blocked / 04:19 clear |
| 5 | `cwf` verify skill | Skip native AS runs whose files are untouched by the delta (e.g. in-room start/stop after a RecordView call-mode-only change) or say why it reran | AS5 in-room reran 11/11, 12/12, 13/13, 14/14 on each head |
| 6 | `workflows/coding` handoff | Persist a `handoffs/*.md` copy per transition (directory missing, so step 4 Communication audit relied on `reason` strings) | `ls <task dir>` shows no `handoffs/` |

Step 8 skipped (automatic mode); nothing applied to clauffice.

## Next part

Spec `plans/2026-11-16-onboarding-spec.md` lists PR3 (first-note tip and Note Styles empty card, AS6-AS7, AS9-AS10; depends on coach route/gate and note-first surface). Branch `feat/onboarding-p3`:

```
cwf init --repo-dir "$HOME/Projects/Scribular/scribular-tauri-onboarding-p3" --spec "$HOME/.local/state/clauffice/workflows/pi0neerpat__scribular-tauri/20261121-feat-onboarding-p2/onboarding-spec.md" --part 3
```
(The spec was copied to the task directory as `onboarding-spec.md` before the clone is deleted. Part 3 needs its coach dependencies merged first.)
