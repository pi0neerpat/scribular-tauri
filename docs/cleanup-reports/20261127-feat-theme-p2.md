# Cleanup report: 20261127-feat-theme-p2

**Summary.** Theme spec part 2 (background pictures, scoped CSP, Stars motion; AS6-AS8, AS11) merged as PR #69, merge commit `e0c9abe0e3e7791f87f588301c26a5fd4c4c56b6` (2026-11-27 22:43Z). Cubic approved the final head `77a8967`. AS11 drag-and-drop stays unverified for Patrick (harness gap: uitest cannot drive an OS drag; the drop shares `import_background_picture` with the AS6 cargo tests). Most of the 15 hours of wall time was waiting: ~755 min human wait, mostly overnight behind a Claude usage limit. Active time was ~178 min. Three lessons: window-only demo recordings and a dedicated recording agent step (Patrick's notes), the verify-session hang on the AS11 file picker, and the overnight usage-limit stall.

Phase settings file: `~/.local/state/clauffice/workflows/phase-settings.json`; `phase_settings_drift`: none.

## Metrics

| Metric | Value |
|---|---|
| Terminal state | merged -> cleanup |
| Active / human-wait / CI-wait | 10711 s (178 min) / 45317 s (755 min) / 1272 s (21 min) |
| Review rounds (cwf counter) | 0 (3 technical_review passes in events: 6bc1b85, c3aad3a, 63a7025) |
| Fix pushes | 4 |
| Compactions / resumes | 1 (build, 06:59) / 0 |
| Interventions | 9 (1 is the cleanup prompt itself, filtered) |
| Premature stops / stop blocks / false ready | 0 / 0 / 0 |
| Cubic threads | 20 on 6bc1b85, 1-3 on c3aad3a, 0 on 63a7025 (22 resolved) |
| Tokens (known sessions, 15/15) | 21.3M total: build 10.8M, verify 9.0M, review 1.5M, cleanup 0.08M; three stalled verify sessions show 0 (no transcript usage) |

Models: build `opus` (claude), verify `sonnet`, review `sonnet`, cleanup `sonnet`.

## Process review

- **Value per session.** Three verify sessions (07:02, 07:22, 07:43) produced no handoff and no verification; the third ran 514 min only because the task sat blocked on the usage limit (events 08:09 `recovery limit reached`, 16:08 human resume). Review of c3aad3a (17:40) found a thread already fixed in code, then a build session (2 min) fixed a different one: the same head was reviewed twice, 17:40 and 17:50. The 4-minute verify at 17:44 re-ran what build had proved. The product-feedback verify re-recorded and also reverted its own `--record-window` work (94c7eed -> 77a8967), so that build session (2.3 min) was wasted by scope.
- **Communication.** The 16:26 review handoff said 20 Cubic threads were blocking ("hook summary wrongly said none"); the build session fixed all 20 but the build handoff noted they were "not yet replied/resolved on GitHub", which the next review had to do. Product feedback asked for a window-only recording; the build handoff implemented it in `tools/uitest`, but the operator note (prompt-0008) had said not to expand uitest in this PR. AS11 was handed to Patrick as unverified-manual with a reason (kept).
- **Time.** Gaps: 199.7 min review->build (waiting for product review, human), 78.3 min review->cleanup (human merge), 8.2 and 5.9 min verify->review (Cubic polling with repeated `reviewer_network_error`, 7+ in a row at 16:18-16:24 and 17:35-17:39). Smooth: build->verify handoffs (0.2 min each). Stalls: usage-limit block 08:09-16:08; AS11 hang 16:31-17:34.
- **Environment gates.** (1) Claude usage limit stalled three verify sessions overnight: `infrastructure` (events 07:22 and 07:43 `session_stale_restart`, 08:09 `blocked`, prompt-0002). (2) AS11 uitest hung in `AXDriver.resolve` (`AXDriver.swift:557`) waiting for 'Choose Picture...' while the Open panel was still on screen (prompt-0006, 0007): `missing_capability` in uitest. (3) The license/trial gate is already handled by the operator note (debug build with `SCRIBULAR_SKIP_LICENSE=1`); no stall recorded.
- **Tokens.** See metrics; Codex not used; stalled sessions unknown, not estimated.
- **Parallel implementation.** The spec's PR split (PR1-PR3) was followed by task; no within-task parallel split was recommended.
- **PR hygiene.** One bot description plus one product-review comment (id 7224861968), re-posted for each head by editing the same comment (no duplicates). The description does not start with a recording filename. Evidence files exist in the task dir (`logs/`, `recordings/`).
- **Demo evidence.** One 99.7 s happy-path mp4, registered after each head change. Patrick rejected the first: full screen with the settings sheet open over the main window for the whole recording, so the theme change was never visible. The re-record closes settings but is still full-screen (window crop was reverted by operator choice). Dead lead-in not measured (the recording was not re-watched by this session).
- **Product review package.** Not re-audited against the "Proof of success" mermaid / "Human Verification Needed" format: the comment starts with "Product review: Theme spec PR2..." but this session did not verify the section names. Unknown; check `cwf package post` output on the next task.

## Classified items

| # | Item | Class | Evidence |
|---|---|---|---|
| 1 | "did you accidentally modify the implementing agents for the other two active workflows" | product_uncertainty (operator question; no cause found) | prompt-0001 |
| 2 | Usage limit reset, continue | infrastructure | prompt-0002, events 07:22-08:09 |
| 3 | Block cleared, resume verify | infrastructure | prompt-0003 |
| 4 | Restart the AS11 verify | missing_capability (uitest hang) | prompt-0004/0005 |
| 5 | "still stuck at selecting an image, Finder open" | missing_capability | prompt-0006 |
| 6 | AS11 hung in AXDriver.resolve, stop and rerun | missing_capability | prompt-0007 |
| 7 | Do not add window-only recording in this PR | product_uncertainty (scope) | prompt-0008 |
| 8 | Review hook summary said no threads; 20 existed | infrastructure (tool/hook) | event 16:26 transition reason |
| 9 | Demo hid the feature behind the settings sheet | bad_instruction (no skill step says to confirm the recording shows the feature) | product-feedback-2026-11-27.md |
| 10 | Cleanup prompt | filtered (task prompt) | prompt-0009 |

## Proposed changes

Per Patrick's operator notes (`cleanup-notes.md`), both are included:

1. **Window-only demo recordings.**
   - Target: `workflows/coding/skills/coding-task/SKILL.md` and `workflows/coding/skills/verify/SKILL.md` (demo recording step), plus `tools/uitest` in the product repo (separate task, as Patrick said).
   - Change: record only the relevant app window(s) via the accessibility tools unless the menu bar or taskbar is part of the demo. Expose a `--record-window` flag in uitest (the reverted 94c7eed is a starting point).
   - Evidence: the first recording showed the full screen with the settings sheet covering the main window (product-feedback-2026-11-27.md); prompt-0008 deferred the capability to a separate task.
2. **Dedicated screen-recording phase.**
   - Target: `workflows/coding/` phase config (`phase-settings.json`) and a new skill, e.g. `skills/record-demo/SKILL.md`.
   - Change: add a phase after verify whose agent understands the implemented feature, records it, then watches and audits the output to confirm it clearly demonstrates the feature before `cwf demo add`.
   - Evidence: verify sessions registered the first recording without watching it (product feedback step 3 had to demand it); the same verify session was also chasing AS11 (16:31-17:34).
3. **uitest picker robustness.**
   - Target: `tools/uitest/Sources/.../AXDriver.swift:557` (product repo, uitest expansion).
   - Change: have `resolve` time out and fail when an NSOpenPanel is up instead of polling labelValues forever; add a scenario step to drive the panel.
   - Evidence: prompt-0006/0007 stack sample.
4. **Usage-limit stalls.** Target: supervisor (`cwf`): when a session exits on a usage-limit message, wait and resume instead of counting it toward `recovery limit`. Evidence: events 07:22, 07:43, 08:09.
5. **Reviewer hook summary.** Target: review skill/hook: make the "no outstanding threads" summary read the live Cubic thread count. Evidence: event 16:26 (20 threads, summary said none).
6. **Handoff to review.** Target: `skills/coding-task/templates/review-package.md`: have build handoffs after a review round state "replied/resolved on GitHub: yes/no" so review does not redo it. Evidence: handoff note at 16:30.
7. **Prompt/phase hygiene.** Target: `coding-task/SKILL.md`: operator notes saying "do not expand X" must be re-read by build before product-feedback work. Evidence: 21:14 build added `--record-window` despite the note given later in prompt-0008 (note arrived after; propose including it at the transition).

Fixes apply to future runs only (symlinked installs pick up committed edits in new sessions). Step 8 skipped (automatic mode).

## Next part

`spec.part` = 2; the approved spec has PR3 (AS9-AS12: Appearance pane, tokens across windows, failure fallback; AS9 manual real-app). Branch `feat/theme-p3`:

```
cwf init --repo-dir "$HOME/Projects/Scribular/scribular-tauri-theme-p3" --spec "$HOME/.local/state/clauffice/workflows/pi0neerpat__scribular-tauri/20261127-feat-theme-p2/theme-spec-copy.md" --part 3
```
