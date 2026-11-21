# uitest: desktop queue and keychain auto-answer

Shipped in PR #44 (spec `plans/2026-11-20-uitest-queue-keychain-spec.md`). Tooling only: `tools/uitest`; no app, Rust or frontend change.

## What it does
- A second `uitest` run waits for the desktop lock instead of failing. It prints `waiting for uitest run pid N`, polls every 2 s, and exits 2 with `uitest lock timeout after <d>: held by pid N` on timeout.
- A Scribular keychain prompt (service `com.app.scribular.tauri`) during launch is answered with the stored login password and "Always Allow".
- With no password, or a prompt still open after answering, the run fails within 15 s: `keychain prompt not answered: ...; fix: bash tools/uitest/trust-build.sh --store-password`. The old 120 s human wait is gone.
- `trust-build.sh` output is saved to `runs/<timestamp>/trust-build.log` (appended across launches in one run) and still echoed.

## How it works
- Lock: `RunLock` (`Sources/UITestCore/RunLock.swift`) polls `flock(LOCK_EX|LOCK_NB)` on `~/Library/Caches/scribular-uitest/uitest.lock` with a monotonic-clock timeout; the descriptor is `O_CLOEXEC`.
- Password: `resolveKeychainPassword` (`Sources/UITestCore/KeychainPassword.swift`) takes `UITEST_KEYCHAIN_PASSWORD` (non-empty), else the item from `/usr/bin/security find-generic-password -s scribular-uitest -a login-password -w`. The value is registered for output redaction and must never reach logs, screenshots or recordings.
- Prompt handling: `Sources/uitest/AXDriver.swift`; trust log: `Sources/uitest/AppLifecycle.swift`.

## Config
| Switch | Effect |
|---|---|
| `--lock-timeout <duration>` | Default `30m`; `0` refuses at once (old behaviour). |
| `UITEST_KEYCHAIN_PASSWORD` | Overrides the stored password. |
| `UITEST_NO_STORED_PASSWORD=1` | Disables stored lookup (prompt tests only). |
| `--no-trust` | Skips `trust-build.sh` so a prompt can be forced (prompt tests only). |

Data: the login password lives in the login keychain (service `scribular-uitest`), stored once with `trust-build.sh --store-password`.

## How to test
- `swift test --package-path tools/uitest` (75 tests: lock wait/timeout/zero, password resolution, option parsing; the lock test holds `flock` from a child process).
- Native: run one scenario twice from two shells and confirm the second waits then passes. For prompts, re-sign a copy of the bundle after changing a resource, launch with `--no-trust`, and confirm the window text appears; repeat with `UITEST_NO_STORED_PASSWORD=1` and confirm the 15 s failure. Grep the run folder for the password: nothing may match.

## Known limits
- macOS may refuse synthetic input into the SecurityAgent field; the 15 s fast failure still applies.
- Only one UI run at a time; other GUI automation on the desktop is still unsupported.
- The stored password is readable by any process running as the user (accepted in `trust-build.sh`).
