# Cleanup report: 20261123-feat-onboarding-p3

## Summary
- **Outcome:** merged. PR #57 (`feat/onboarding-p3`) merged at `0442e6c09121925c778ffeb87826fffd7c2c9518` on 2026-11-23 20:35Z. Onboarding spec PR3 (AS6, AS7, AS9, AS10) shipped. It is the last part of the spec, so there is no next-part command.
- **Unverified (for Patrick):** AS9 first-microphone-prompt timing, a note from live microphone speech, and production trial initialisation. The VM already had a mic grant and returned "insufficient audio". The clean-profile app-audio fixture run did generate a note and show the tip (29/29).
- `cargo fmt --check` failed on one assertion in `queue_coach_tests.rs` (events 66, 76); the PR comment says `origin/main` has the same formatting. Not fixed in this task.
- Cubic: 3 threads on 5c0c9e0, all resolved. 1 fix push cycle. Approved on 979bfd5.
- Docs for the shipped solution and this report went into a docs PR (see PR link in the final message).

## Metrics
| Metric | Value |
|---|---|
| Active / human-wait / CI-wait | 4176 s / 454 s / 661 s |
| Review rounds / fix pushes | 0 / 2 |
| Compactions / resumes | 0 / 0 |
| Premature stops / stop blocks / false ready | 0 / 0 / 0 |
| Interventions | 1 (the cleanup prompt) |
| Unverified manual / demos / stale demos | 1 / 1 (happy-path) / 0 |

Sessions (from `session_tokens.py`; Codex counts are estimates):
| Phase | Harness / model | Wall | Total tokens |
|---|---|---|---|
| build | codex / gpt-6.2-sol | 22.5 min | 15.27 M |
| verify | codex / gpt-6.2-sol | 18.0 min | 5.38 M |
| review | codex / gpt-6.2-sol | 1.7 min | 0.52 M |
| build (fix) | codex / gpt-6.2-sol | 2.7 min | 0.87 M |
| verify | codex / gpt-6.2-sol | 24.1 min | 7.91 M |
| review | codex / gpt-6.2-sol | 2.2 min | 0.86 M |
| cleanup | claude / sonnet | n/a | 0.14 M (first turn only) |

Total about 31 M tokens (build 16.1 M, verify 13.3 M, review 1.4 M). Idle gaps: 4.0, 3.0 and 8.2 min (review→cleanup, includes the human merge).

## Classified items
| Item | Class | Evidence |
|---|---|---|
| Intervention: cleanup prompt | not a failure (supervisor-issued task prompt) | event 121, `logs/prompt-0001.txt` |
| Codex usage limit 1 min into build | `infrastructure` | events 5-8; auto-resumed 11:18 PT after about 8 min |
| Release build "Trial ended" licence gate | `infrastructure` | `watcher-config.json` `operator_note`; every verify used the debug bundle with `SCRIBULAR_SKIP_LICENSE=1`, so no stall |
| Cubic network errors while polling | `infrastructure` | events 101, 103; recovered by event 104 |
| Live-mic AS9 not verifiable on the VM | `infrastructure` | handoffs e30814ef and 52cd9625 `unverified` |
| Premature stops, false-ready | none | report metrics are zero |

## Process review
- **Value per session.** Build, verify and the final review each earned their time. Verify #1 (18 min) caught nothing new and Cubic found 3 threads on that head; the second build was a 2.7 min fix. Verify #2 (24 min, 7.9 M tokens) re-ran the whole AS9 matrix (labels, fixture, three demo recordings) after a change that only touched test log guards and a test assertion in Rust. Overlap: the "AS9-demo" run was recorded three times in each verify (events 36-39, 88-90). Proposed: scope re-verification to the changed paths.
- **Communication.** The AS9 `unverified` entry carried through every handoff intact. The "cargo fmt fails, same on main" note is only in the PR evidence block and the verification events, not in a handoff. No dropped findings seen. Note: the first review handoff (e30814ef) says "Build first action", and build session 2 did exactly those two Cubic threads.
- **Time.** Build 1 22.5 min, verify 1 18 min, review 1.7 min, build 2 2.7 min, verify 2 24 min, review 2 2.2 min. Gaps are small. The only stall was the usage limit (about 8 min). Environment gates (licence, keychain) did not stall verify because the operator note was in place before start.
- **Tokens.** See table. Cleanup session usage is partial (known first turn only).
- **Parallel implementation.** The spec did not recommend splitting; one build agent did the work.
- **PR hygiene.** One product-review comment from pi0neerpat plus the Cubic summary, no duplicates. The recording is a local path, not a top-of-description filename.
- **Demo evidence.** One happy-path demo, named by spec (`...onboarding-spec-p3-happy-path.mp4`). Dead lead-in before the app window opens was not measured here (video not inspected). It shows the first-note tip on the app-audio fixture, not the mic path, which the package states.
- **Product review package.** Non-compliant on three points: (1) the recording block is titled "Recording", not "Proof of success"; (2) there is no mermaid user-journey diagram; (3) the section is named "Test it yourself", not "Human Verification Needed". Its steps are numbered, one action each, and the single AS9 item genuinely needs a human, so that part is fine.

## Proposed changes
1. **Target:** `workflows/coding/skills/coding-task/templates/review-package.md` (headings at lines 9 and 13). **Change:** rename "Test it yourself" to "Human Verification Needed" and "Recording" to "Proof of success", and add a mermaid journey diagram placeholder under "Proof of success". **Evidence:** PR #57 comment 7113087512 uses the old headings and has no diagram.
2. **Target:** the coding-task verify skill/prompt (re-verification after a fix push). **Change:** after a review-fix push, re-run only the suites whose paths the fix touched; re-record a demo only if UI code changed. **Evidence:** second verify (events 73-91, 24 min, 7.9 M tokens) re-ran all AS9 scenarios and three demo recordings for a Rust test-only fix (events 67, `979bfd5` touches `diag/log.rs`, `diag/telemetry.rs`, `queue_coach_tests.rs`).
3. **Target:** verify skill, same file. **Change:** a recorded demo is retried at most once without a stated reason in the handoff. **Evidence:** three AS9-demo runs per verify (events 36-39, 88-90).
4. **Target:** `watcher-config.json` `operator_note` (this repo). **Change:** add: "The VM already holds a microphone grant and returns insufficient audio; AS9 live-mic steps are Human Verification Needed, do not retry them." **Evidence:** handoffs e30814ef and 52cd9625, `AS9.uitest` timeout.
5. **Target:** project docs (`tools/uitest/README.md`). **Change:** note that `cargo fmt --check` fails on `queue_coach_tests.rs` on main, or fix it in a docs-excluded follow-up. **Evidence:** events 66, 76; PR evidence block.
