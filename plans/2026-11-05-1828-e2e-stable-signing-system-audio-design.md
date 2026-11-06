# E2E testing: stable local signing + system-audio recording tests — design

## Why

During PR #10's final stages two things blocked full E2E runs of the `uitest` tool (`tools/uitest`):

1. **Keychain prompt after every rebuild.** This Mac has no code-signing identity (`security find-identity -v -p codesigning` → 0), so every build is ad-hoc signed. macOS binds keychain ACLs ("Always Allow") and privacy grants (TCC) to the code signature; an ad-hoc signature's designated requirement is the binary's hash, so each rebuild is treated as a new app. The DB key lives in the login keychain (`src-tauri/src/store/key.rs:197`, `KeychainKeyStore`), so the rebuilt app blocks on a SecurityAgent prompt. The uitest workaround (typing the login password via `UITEST_KEYCHAIN_PASSWORD`, `tools/uitest/Sources/uitest/AXDriver.swift:25-34`) has needed four fix commits and still treats the symptom.
2. **AS11 can't record.** Repeated attempts to feed audio through the microphone path failed: you can't reliably play audio and record it through the mic on the same machine. The user decided (2026-11-05) to **stop testing via mic** and use the **system-audio ("App audio") path** instead.

The same signature problem would also hit system-audio capture: the "System Audio Recording" TCC grant is bound to the signature too, so it must be fixed for the audio path to be stable.

## Goals

- Rebuilding the app and re-running any uitest scenario needs **no keychain prompt and no permission prompt** after a one-time setup.
- uitest drives the **real shipping build** (no `fake-engines`): real system audio → real Whisper → real Gemma summary. This is the standard for all recording scenarios.
- Recording scenarios prove audio really arrived by asserting a **known phrase from the fixture** in the transcript (a silent capture must fail).
- **No uitest run ever re-downloads models** once they're cached (user requirement, applies to every run and every scenario).
- Future agents know: **recording tests use system audio, never the mic.**

## Non-goals

- Production / release signing and notarization (separate work; do not touch `tauri.conf.json` signing for release).
- Testing mic capture automatically. Mic stays a manual pre-release check.
- Asserting summary wording (LLM output varies); only that a summary exists and which style produced it.
- Removing the `fake-engines` Cargo feature: `cargo test` still uses it (job/queue/diarize unit tests, `src-tauri/src/diarize.rs`). It just stops being the uitest build.
- Changing the app's key storage (no file-backed test keystore).

## Anti-goals

- Anything that weakens the shipped app's security (e.g. a keychain bypass compiled into release builds).
- Committing a private key or password to the repo.
- Leaving the old password-typing path as the expected route; it may stay as a fallback but docs must not tell people to rely on it.

## Decisions (user's)

- **Out of scope: rewriting the AS11 scenario** (user dropped it 2026-11-05). Existing scenarios aren't migrated in this work beyond removing `handle-keychain`.

- **Use a local self-signed code-signing certificate** (no Apple Developer account).
- Drop the mic path for automated tests; harden the system-audio path.
- uitest runs the real app build, not `fake-engines` (chosen over "real STT + fake summary" and "fake engines + silence check").
- A shared model cache prevents re-downloads on **all** uitest runs.
- Document the system-audio rule for future agents.

## Design

### 1. Stable local signing

- One-time script `tools/uitest/setup-signing.sh` (idempotent):
  - Creates a self-signed **code signing** certificate named `Scribular Local Dev` in the login keychain if it doesn't exist (via `openssl` to make cert + key, `security import` a .p12, and `security add-trusted-cert -p codeSign` so `codesign` accepts it; may prompt once for the login password — that's acceptable in a one-time setup).
  - Prints the identity and the export line for builds.
- Drop `handle-keychain` from scenarios once signing is in place; keep `keychain-after-rebuild.uitest` but change it to "rebuild, relaunch, expect no prompt" (`expect "Settings"` without `handle-keychain`).
- Builds for testing pass the identity via env var, not config: `APPLE_SIGNING_IDENTITY="Scribular Local Dev" npm run tauri -- build --bundles app` (no `--features fake-engines`; update `Preflight.swift:21`'s hint to match). Tauri signs the app and its sidecar (`AudioCapCLI`) with it. `tauri.conf.json` is not changed.
- After the first launch of a signed build, the user clicks "Always Allow" on the keychain prompt and grants System Audio Recording once. Subsequent rebuilds keep the same designated requirement (cert leaf + bundle ID), so no further prompts.
- Verification: `codesign -dr - <app>` shows a requirement referencing the certificate, not a `cdhash`. **Check the sidecar too** (`Contents/MacOS/AudioCapCLI`): the System Audio Recording grant may attach to it. If Tauri leaves it ad-hoc signed, add an explicit `codesign --force -s "Scribular Local Dev"` step for it (and re-sign the app after).
- **Spike first (Claude's call):** before building the rest, prove that grants survive: sign, launch, approve keychain + audio once, rebuild, relaunch, and confirm no prompts. If prompts come back, **stop and report to the user**. Don't work around it.
- **One-time setup is done by the user at the machine (Claude's call):** it covers the cert trust prompt (login password), the first "Always Allow" on the existing keychain item (its ACL still names the old ad-hoc hash, so one prompt is expected), and the System Audio Recording grant. No `tccutil reset` and no keychain item deletion.
- Build output path on this machine: `/Users/admin/cargo-target/scribular-tauri/release/bundle/macos/Scribular.app` (from `tools/uitest/README.md`). Signed rebuilds replace that same path, and `--app` points at it.

### 2. Shared model cache (all uitest runs)

- Models live in `<app_data_dir>/models` (`src-tauri/src/lib.rs:180`). A default `launch` moves the whole app data dir into the run folder (`tools/uitest/Sources/UITestCore/DataDir.swift:8`), so today every fresh run re-downloads ~150 MB of Whisper plus ~1.7 GB of Gemma. That's why AS11 waits `timeout 300s` on "Getting Scribular ready".
- Fix in uitest only, with no app change. After moving the data dir and before starting the app, `launch` creates a fresh `<app_data_dir>/models` that points at a persistent cache, `~/Library/Caches/scribular-uitest/models`. Use a **symlink** (Claude's call): the first run's downloads land straight in the cache, and later runs find the files already there. If the app doesn't follow the symlink (e.g. it removes or replaces the dir), fall back to APFS clones (`FileManager.copyItem`) in, and copy new files back to the cache after the run.
- `launch --keep-data` doesn't move anything, so it's unaffected.
- The cache is outside the repo and never committed. The README documents it and how to clear it (`rm -rf ~/Library/Caches/scribular-uitest`).
- Verify: on the second fresh run, `launch` to "ready" takes seconds, not minutes. The `expect-not "Getting Scribular ready" timeout 300s` line can then drop to a short timeout, and the first-ever run is the only slow one.
- Unit-test the link/clone helper in `UITestCoreTests`, like `moveDataDir`.

### 3. uitest `play` command

- New script command `play "relative/or/absolute.wav" [loop]`: starts `/usr/bin/afplay <file>` in the background; the process is killed when the run ends (success, failure, or `quit`), and any prior `play` is stopped first. Paths resolve relative to the repo root (or the scenario file — builder's call, document it).
- `loop` restarts afplay when it exits, so a 12s fixture covers a 30s recording.
- Fixture: reuse `src-tauri/tests/fixtures/audio/2-speakers-12s.wav`.
- Parser + runner unit tests in `tools/uitest/Tests/UITestCoreTests` like the existing commands; README command table updated.

### 4. Documentation for future agents

- New repo-root `CLAUDE.md` (none exists) with a short "E2E / uitest" section: build the real app (no `fake-engines`) with `APPLE_SIGNING_IDENTITY="Scribular Local Dev"`; recording tests use App audio + `play`, never the mic; assert fixture phrases, never summary wording; models come from the shared cache, so never delete it as a "fix"; pointer to `tools/uitest/README.md`.
- `tools/uitest/README.md`: one-time setup gets the signing script step (replacing the `UITEST_KEYCHAIN_PASSWORD` step as the primary path; mention the env var only as a fallback); build command includes the identity; "Recording tests" subsection stating the system-audio rule.
- Document the recording-scenario pattern for future scenario authors: `play "<fixture>" loop` → Settings → `App audio` → `Refresh` (retry if `afplay` isn't listed yet) → select `afplay` in the `App` dropdown → record → `expect` a known fixture phrase in the transcript (pick one that transcribes identically across runs) → `expect` the summary header. Never assert summary wording.

## Constraints

- macOS 14+, Apple Silicon (helper binary is `AudioCapCLI-aarch64-apple-darwin`).
- Bundle output path stays the same; only the build command changes (drop `--features fake-engines`, add the signing env var).
- First-ever run needs network for the ~1.85 GB model download; every later run must work from the cache.
- Commits pushed to Forgejo per repo rules.

## Left to the builder

- Exact openssl/security incantation for the certificate (must yield an identity listed by `security find-identity -v -p codesigning`).
- How `play` paths resolve and how the child is tracked/killed in the Swift driver.
- How to drive the native `<select>` through Accessibility (needed only to verify `play` end-to-end once; document what works).
- Whether `handle-keychain` code stays as a fallback or is removed (keep if cheap).
