# Cleanup report: 20261120-feat-onboarding-p1

**Outcome: merged.** PR #43 (`feat/onboarding-p1`, head `bb67cf9`) merged to `main` at `c7086baf14014f1ed7fd90c4e6bff8e12d3966ae` on 2026-11-20 23:18Z. Scope: PR1 of the onboarding split (AS1-AS3): one welcome sheet with persisted consent, FirstRun/tour removal, speech-model status and retry on Record.

**Summary.** A clean run: one build, two verifies, two reviews, one fix round (four Cubic threads, all fixed in one 3-minute build session at `bb67cf9`). No real interventions, premature stops, compactions, resumes or false-ready reports. The only large time cost was a Codex usage limit that parked verify for about 2h43m (infrastructure). The weak spots are all in the review package (section names, no mermaid diagram, later-PR manual items listed) and in verify evidence kept in `/private/tmp`, which no longer exists.

Models: build/verify/review ran on Codex `gpt-6-sol` (events record `model: None`; the model comes from the token script); cleanup runs on Claude `sonnet`.

## Metrics

| Metric | Value |
|---|---|
| Terminal state | cleanup (PR merged) |
| Active / human-wait / CI-wait | 29,018 s (8.06 h, includes the 2h43m usage-limit park) / 34 s / 722 s |
| Review rounds / fix pushes | 2 / 1 (cwf counts 2; the second push is the same `bb67cf9`) |
| Compactions / resumes (cwf) | 0 / 0 (one usage-limit auto-resume, events 15-16, not counted as a resume) |
| Interventions | 1 recorded, 0 real (it is the cleanup prompt itself, `logs/prompt-0001.txt`, filtered) |
| Premature stops / stop blocks / false ready | 0 / 0 / 0 |
| Demos | 1 happy-path, not stale |
| Unverified manual | 2 (AS4, AS9), both belong to later PRs |

Tokens (Codex counts are estimated; cleanup is Claude, partial because still running): build 5.24M, verify 22.88M, review 1.45M, other 0.14M, total 29.7M. Verify dominates: the first verify session ran 184 min wall (about 20 min of work plus the limit park) and read 15.9M input tokens, mostly cached.

## Process review

- **Value per session.**
  - Build 1 (9.2 min): produced the full PR1 with Vitest and Rust tests. Useful.
  - Verify 1 (184 min wall): useful; paid for it with the usage-limit stall and a disk-full on `store::settings` (verification entries 5 and 10; fixed by clearing regenerable caches).
  - Review 1 (1.9 min): useful; found four real Cubic threads and handed them to build with a precise note.
  - Build 2 (2.9 min): fixed all four at `bb67cf9`. Useful.
  - Verify 2 (16 min): reran native, help and demo suites on the new head, which was required because the head changed. Overlap: build 2 had already run full vitest, cargo and `npm run check` at `bb67cf9` (evidence dir), and verify 2 reran vitest and `npm run check` again (focused). Small, cheap.
  - Review 2 (3 min): Cubic P2 on `src/App.svelte` was already addressed; the agent replied with evidence and resolved the thread. This was a re-review of a finding that build 2 had already fixed, so the thread was stale rather than the code wrong.
  - No same-head double review, no two sessions fixing the same finding.
- **Communication.** Build 1 -> verify 1 and build 2 -> verify 2 handoffs were followed item by item. Dropped or weak items:
  - The verify 2 handoff says `cwf screenshot` could not find a visible app window in the tmux session; uitest screenshots were used instead. The next session did not need it, but nothing records a fix.
  - Verify 1's handoff cites logs under `/private/tmp/scribular-onboarding-verify/`; that directory is gone now, so 14 of the recorded verification artifacts cannot be opened (see Proposed changes 2).
- **Time.** Sessions (wall): build 9.2 min, verify 184 min, review 1.9, build 2.9, verify 16, review 3.0. Idle gaps: 0.01 / 5.5 / 0.01 / 0.1 / 1.7 min between sessions, then **271.6 min between review 2 and the cleanup session**. The merge happened 34 s after the package; the cause of the 4.5 h wait before cleanup started is unknown (no event explains it). Environment gates that stalled verify:
  - `session_usage_limit` (events 11-16, 2026-11-20 19:45Z): Codex hit its limit 2 min into verify; auto-resumed at 14:28 PT. Classified `infrastructure`; no fix possible beyond the existing auto-resume.
  - Disk-full during `cargo test store::settings` (verify 1 handoff). `infrastructure`.
  - The license/trial gate named in the operator note did not stall this run: every uitest used a debug bundle with `SCRIBULAR_SKIP_LICENSE=1` from the start. The note worked.
- **Tokens.** See Metrics. Per-session numbers come from `session_tokens.py`; Codex numbers are estimates, cleanup session usage is partial.
- **Parallel implementation.** The spec split work into three sequential PRs, not parallel agents. Nothing recommended splitting PR1 across agents; one agent built it.
- **PR hygiene.** One PR comment (the product review package, `<!-- cwf:agent-reply -->`), no duplicates. The PR description starts with `Recording: <local .mp4 path>` at the very top, which is the pattern the skill flags. Evidence in the description cites `/private/tmp/...` UI-test scripts that no longer exist (the `evidence/` dir holds only the build-session logs).
- **Demo evidence.** One happy-path recording, 18 s, named by spec (`...onboarding-spec-p1-happy-path.mp4`), shows the welcome sheet, Continue and Record, so it shows the feature rather than a license gate. 18 s makes a long dead lead-in impossible; I did not decode frames. The first recording attempt on `bb67cf9` failed (verification at 23:02:53Z) and passed on rerun at 23:04Z; the failure log is gone, so the cause is unknown.
- **Product review package.** Fails the template on three points: the recording block is titled "Recording" not "Proof of success"; there is no mermaid user-journey diagram; the section is named "Test it yourself" not "Human Verification Needed". It also lists AS4 and AS9, which are PR2/PR3 items that cannot be tested on this PR, each as a single paragraph rather than numbered one-action steps.

## Classified items

| Item | Class | Evidence |
|---|---|---|
| Intervention at 03:49:37Z | filtered (task notification / cleanup prompt) | `logs/prompt-0001.txt` is this cleanup prompt |
| Verify 1 parked 2h43m | `infrastructure` | events 11-16, `session_usage_limit` and `session_usage_resume` |
| Disk-full in `store::settings` test | `infrastructure` | verification entry 5 (fail) then entry 10 (pass) after cache cleanup |
| First demo recording failed, rerun passed | unknown (log missing) | verification entries at 23:02:53Z and 23:04:16Z |
| Package section names, no mermaid, no "Proof of success" | `bad_instruction` or `model_error` (template check needed) | `review-package.md` headings |
| Later-PR manual items AS4, AS9 listed in PR1 package | `bad_instruction` (cwf/skill does not scope manual items to the current spec part) | `unverified_manual` in `cwf status`, package body |
| `cwf screenshot` found no window in tmux | `missing_capability` | verify 2 handoff note |
| Verify artifacts under `/private/tmp` lost | `bad_instruction` (skill allows artifacts outside the task dir) | `cwf status` verification artifact paths vs `evidence/` |
| 271 min wait before cleanup started | unknown | no event between 23:18:32Z and 03:49:31Z |

## Proposed changes

All proposals target the canonical source `workflows/coding/` in the clauffice repo and apply to future runs only. None were applied (automatic mode, step 8 skipped).

1. **`skills/coding-task/templates/review-package.md`**: rename "Test it yourself" to "Human Verification Needed" and "Recording" to "Proof of success"; add a required mermaid `journey` or `flowchart` block of the demo's user path; require numbered one-action steps per item. Evidence: this package's headings and AS4/AS9 paragraphs.
2. **`skills/coding-task/SKILL.md`, verify section (around lines 107-135)**: require uitest scripts, logs and `--record-dir` output to be written to `<task dir>/evidence/` (or copied there before handoff), not `/tmp`. Evidence: 14 verification artifacts under `/private/tmp/scribular-onboarding-verify/` no longer exist.
3. **`skills/coding-task/SKILL.md`, handoff section (around lines 111-132)**: when the spec has parts, record `--unverified` manual-only items only for the AS ids in the current part, and keep later-part items out of the review package. Evidence: AS4 and AS9 ("belongs to PR2/PR3") in the PR1 package and `unverified_manual_count: 2`.
4. **`skills/coding-task/SKILL.md`, PR description rule**: put the PR scope first and the recording last; do not put a local file path at the top. Evidence: PR #43 body starts with `Recording: ~/.local/state/...mp4`.
5. **`cwf screenshot` (`workflows/coding` source)**: fall back to a full-screen `screencapture` or the last uitest screenshot when no app window is visible in a tmux session. Evidence: verify 2 handoff note.
6. **Supervisor**: log why cleanup starts hours after merge (queueing, sleep, or manual start) so the idle gap is explainable. Evidence: 271.6 min gap with no events.

## Next part

`spec.part` is 1 and the approved spec (`plans/2026-11-16-onboarding-spec.md`) has PR2 (AS4-AS5, AS8; first On a call explainer, OS permission status). Next branch: `feat/onboarding-p2`. Start it with a fresh clone:

```
cwf init --repo-dir "$HOME/Projects/Scribular/scribular-tauri-onboarding-p2" --spec "$HOME/Projects/Scribular/scribular-tauri-onboarding/plans/2026-11-16-onboarding-spec.md" --part 2
```

The spec lives in the clone that is deleted below; it is also on `origin/main`, so use the fresh clone's copy of `plans/2026-11-16-onboarding-spec.md` if you adjust the path.
