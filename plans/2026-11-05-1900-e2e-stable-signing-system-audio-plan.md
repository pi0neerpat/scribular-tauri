# E2E: stable local signing, shared model cache, system-audio `play` — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** uitest runs the real signed app build across rebuilds with no keychain/permission prompts, never re-downloads models, and can feed real audio through the App-audio path via a new `play` command.

**Architecture:** A one-time script creates a self-signed `Scribular Local Dev` identity; test builds are signed with it via `APPLE_SIGNING_IDENTITY`, so macOS keychain ACLs and TCC grants stay valid across rebuilds. uitest's `launch` symlinks the fresh app-data `models` dir to a persistent cache. A new `play` script command runs `/usr/bin/afplay` in the background so the app can capture it in App-audio mode.

**Tech Stack:** Swift 6 package `tools/uitest` (swift-testing), bash + `openssl`/`security`/`codesign`, Tauri 2 build.

**Spec:** `plans/2026-11-05-1828-e2e-stable-signing-system-audio-design.md`

## Global Constraints

- Identity name: `Scribular Local Dev`. Build: `APPLE_SIGNING_IDENTITY="Scribular Local Dev" npm run tauri -- build --bundles app` (no `--features fake-engines`).
- App bundle: `/Users/admin/cargo-target/scribular-tauri/release/bundle/macos/Scribular.app`; bundle id `com.app.scribular.tauri`.
- Model cache: `~/Library/Caches/scribular-uitest/models`. App models dir: `~/Library/Application Support/com.app.scribular.tauri/models` (`src-tauri/src/lib.rs:180`).
- Do not change `tauri.conf.json`, the app's key storage, or the `fake-engines` Cargo feature (still used by `cargo test`).
- Never commit a private key, `.p12`, or password.
- Out of scope: rewriting `as11-note-style-default.uitest` or migrating other scenarios.
- Recording tests use App audio + `play`, never the mic.
- After every commit: `git push` (remote is GitHub `pi0neerpat/scribular-tauri`; report, don't change it).

## Review Focus

1. A real (non-symlink) `models` dir already in app data from a manual app run, with an empty cache → must seed the cache from it, not move 1.85 GB into the run folder and re-download. (Task 2 test `seedsCacheFromExistingModels`.)
2. `models` is already a symlink to the cache (data moved from a previous uitest run, or `--keep-data`) → no error, no double link, cache untouched. (Task 2 test `linkIsIdempotent`.)
3. A run fails mid-scenario while `play` is running → `afplay` must not keep playing after uitest exits. (Task 3 manual step 6.)
4. `play` path missing → step fails immediately with the resolved path, not a silent no-op that later looks like a capture bug. (Task 3 test `playMissingFileFails` via `resolvePlayPath`.)
5. Cache dir contains a partial download from an interrupted run → linking must never delete or rewrite cache contents; the app's own verify handles bad files. (Task 2 test `linkKeepsCacheContents`.)

---

### Task 1: Stable local signing (spike gate)

**Files:**
- Create: `tools/uitest/setup-signing.sh`
- Modify: `tools/uitest/scenarios/keychain-after-rebuild.uitest`
- Modify: `tools/uitest/Sources/uitest/Preflight.swift:21` (build hint string)
- Modify: `tools/uitest/README.md` (One-time setup, Build and run)

**Interfaces:**
- Produces: a codesigning identity `Scribular Local Dev` in the login keychain; the build command in Global Constraints.

- [ ] **Step 1: Write `setup-signing.sh`** (bash, `set -euo pipefail`, idempotent). If `security find-identity -v -p codesigning | grep -q "Scribular Local Dev"` → print "already set up" and exit 0. Otherwise in a `mktemp -d` dir (deleted on exit via `trap`): `openssl req -x509 -newkey rsa:2048 -nodes -days 3650 -subj "/CN=Scribular Local Dev" -addext "keyUsage=critical,digitalSignature" -addext "extendedKeyUsage=critical,codeSigning"`, bundle to `.p12` with a random throwaway passphrase (`openssl pkcs12 -export -legacy`), `security import <p12> -k ~/Library/Keychains/login.keychain-db -P <pass> -T /usr/bin/codesign`, then `security add-trusted-cert -r trustRoot -p codeSign -k ~/Library/Keychains/login.keychain-db <cert.pem>`. End by printing the build command.
- [ ] **Step 2: User runs it** (needs the login password once): `bash tools/uitest/setup-signing.sh`. Expected: `security find-identity -v -p codesigning` lists `"Scribular Local Dev"`.
- [ ] **Step 3: Build signed real app** from repo root with the Global Constraints build command. Expected: build succeeds.
- [ ] **Step 4: Check both signatures.** `codesign -dr - <app>` and `codesign -dr - <app>/Contents/MacOS/AudioCapCLI*`. Expected: each designated requirement contains `certificate leaf = H"…"` and no `cdhash`. If the sidecar is ad-hoc: add to README a post-build step `codesign --force -s "Scribular Local Dev" <sidecar> && codesign --force -s "Scribular Local Dev" --entitlements src-tauri/entitlements.plist <app>`, re-run this check.
- [ ] **Step 5: Spike — user at the Mac.** Launch the app; click "Always Allow" on each keychain prompt; in Settings choose `App audio`, play `afplay src-tauri/tests/fixtures/audio/2-speakers-12s.wav`, Refresh, pick `afplay`, record 5 s, grant System Audio Recording. Quit. Rebuild (Step 3, after `touch src-tauri/src/lib.rs`), relaunch, record again. Expected: **no keychain prompt and no audio permission prompt** on the second launch. **If any prompt returns: stop and report to the user.** Do not continue to Tasks 2–4.
- [ ] **Step 6: Update `keychain-after-rebuild.uitest`** to `launch --keep-data` / `expect "Settings"` / `quit` (no `handle-keychain`). Run it against the rebuilt app: `swift run --package-path tools/uitest uitest tools/uitest/scenarios/keychain-after-rebuild.uitest --app <app>`. Expected: final line reports pass.
- [ ] **Step 7: Update Preflight hint** to the Global Constraints build command; update README One-time setup (step: run `setup-signing.sh`; `UITEST_KEYCHAIN_PASSWORD` demoted to "fallback only, not needed with signed builds") and Build and run (new command, sidecar step if Step 4 needed it). Run `swift build --package-path tools/uitest`. Expected: builds.
- [ ] **Step 8: Commit + push**
```bash
git add tools/uitest/setup-signing.sh tools/uitest/scenarios/keychain-after-rebuild.uitest tools/uitest/Sources/uitest/Preflight.swift tools/uitest/README.md
git commit -m "feat(uitest): stable local signing identity for prompt-free rebuilds" && git push
```

### Task 2: Shared model cache

**Files:**
- Modify: `tools/uitest/Sources/UITestCore/DataDir.swift`
- Modify: `tools/uitest/Sources/uitest/AppLifecycle.swift:36-45`
- Test: `tools/uitest/Tests/UITestCoreTests/DataDirTests.swift`
- Modify: `tools/uitest/README.md` (new "Model cache" subsection)

**Interfaces:**
- Produces: `public func linkModelCache(appDataDir: URL, cache: URL) throws` in `DataDir.swift`.
  Behavior: create `cache` (with intermediates) if missing. If `appDataDir/models` is a symlink to `cache` → return. If it is a real directory: move each file inside it into `cache` unless a same-named file already exists there, then remove the now-redundant dir. Create `appDataDir` if missing, then symlink `appDataDir/models` → `cache`. Never delete or overwrite anything inside `cache`.

- [ ] **Step 1: Write failing tests** in `DataDirTests.swift`, same temp-dir style as `moveDataDirMovesContents`:
  - `linksFreshDataDir`: no app-data dir → after call, `destinationOfSymbolicLink(atPath: app/models) == cache.path`.
  - `seedsCacheFromExistingModels`: `app/models/w.bin` real file, empty cache → `cache/w.bin` exists with same bytes, `app/models` is a symlink.
  - `linkIsIdempotent`: call twice → no throw, still a symlink to cache.
  - `linkKeepsCacheContents`: `cache/w.bin` = "old", `app/models/w.bin` = "new" → `cache/w.bin` still "old".
- [ ] **Step 2: Run** `swift test --package-path tools/uitest --filter DataDirTests`. Expected: FAIL, `linkModelCache` not found.
- [ ] **Step 3: Implement `linkModelCache`** with `FileManager` (`createSymbolicLink(at:withDestinationURL:)`, `destinationOfSymbolicLink`, `attributesOfItem` `.type == .typeSymbolicLink` to detect the link — `fileExists` follows links).
- [ ] **Step 4: Run** the same command. Expected: PASS.
- [ ] **Step 5: Wire into `launch`.** In `AppLifecycle.launch`, before `moveDataDir`, call `linkModelCache(appDataDir: source, cache: <cache>)` so an existing real models dir seeds the cache instead of moving into the run folder; after `moveDataDir`, call it again so the fresh (absent) data dir gets the link. Also call it when `keepData` is true. Wrap errors as `.fatal("could not link model cache: …")`. Cache URL: `homeDirectoryForCurrentUser/Library/Caches/scribular-uitest/models`.
- [ ] **Step 6: Verify against the real app.** Run `keychain-after-rebuild.uitest` but with plain `launch` plus `expect-not "Getting Scribular ready" timeout 900s` twice in a row (temporary scratch scenario in the scratchpad, not committed). Expected: first run downloads (or uses seeded cache); second run clears "Getting Scribular ready" in under 30 s; `ls -la ~/Library/Application\ Support/com.app.scribular.tauri/models` shows a symlink. If the app ignores or replaces the symlink, switch Step 3 to APFS clones (`copyItem`) in and copy-back of new files after quit, keeping the tests' assertions on the cache.
- [ ] **Step 7: README** "Model cache" subsection: location, first run downloads ~1.85 GB, later runs reuse it, clear with `rm -rf ~/Library/Caches/scribular-uitest`, don't clear it to "fix" a test.
- [ ] **Step 8: Commit + push**
```bash
git add tools/uitest/Sources/UITestCore/DataDir.swift tools/uitest/Sources/uitest/AppLifecycle.swift tools/uitest/Tests/UITestCoreTests/DataDirTests.swift tools/uitest/README.md
git commit -m "feat(uitest): share a model cache across runs" && git push
```

### Task 3: `play` command

**Files:**
- Modify: `tools/uitest/Sources/UITestCore/Script.swift` (enum + parser)
- Create: `tools/uitest/Sources/UITestCore/Play.swift` (path resolution)
- Create: `tools/uitest/Sources/uitest/Player.swift` (afplay child)
- Modify: `tools/uitest/Sources/uitest/AXDriver.swift` (`perform` cases), `tools/uitest/Sources/uitest/main.swift` (stop after `runSteps`)
- Test: `tools/uitest/Tests/UITestCoreTests/ScriptTests.swift`, `tools/uitest/Tests/UITestCoreTests/PlayTests.swift`
- Modify: `tools/uitest/README.md` (command table)

**Interfaces:**
- Produces: `Command.play(path: String, loop: Bool)`; `Command.stopAudio`; `public func resolvePlayPath(_ path: String, cwd: URL) throws(DriverError) -> URL` (relative paths resolve against `cwd`; missing file → `.fatal("play: file not found: <absolute path>")`); `final class Player { func start(_ file: URL, loop: Bool) throws; func stop() }`.

- [ ] **Step 1: Write failing tests.**
  - `ScriptTests`: `play "a.wav"` → `.play(path: "a.wav", loop: false)`; `play "a.wav" loop` → `loop: true`; `play` with no arg, unquoted path, or extra word → `ScriptError` with message `usage: play "file" [loop]`; `stop-audio` → `.stopAudio`; `play "a.wav" timeout 5s` → error (not in the timeout list).
  - `PlayTests`: `playResolvesRelativeToCwd` (temp file) → absolute URL; `playMissingFileFails` → throws `.fatal` whose message contains the absolute path.
- [ ] **Step 2: Run** `swift test --package-path tools/uitest`. Expected: FAIL to compile (no `.play`).
- [ ] **Step 3: Implement** enum cases, parser branch (pattern of `screenshot`/`type`), `stop-audio` in the no-arg branch, `resolvePlayPath` in `Play.swift`.
- [ ] **Step 4: Run** `swift test --package-path tools/uitest`. Expected: PASS (all existing tests too).
- [ ] **Step 5: Implement `Player`** with `Process` running `/usr/bin/afplay <file>`; `start` stops any current playback first; with `loop`, a `terminationHandler` restarts it unless `stop()` was called. AXDriver holds one `Player`: `.play` → `resolvePlayPath(path, cwd: FileManager.default.currentDirectoryPath)` then `start`; `.stopAudio` and `.quit` → `stop()`. `main.swift`: call `driver.stopAudio()` (a small public method wrapping `player.stop()`) right after `runSteps` returns, pass or fail. Also kill on SIGINT using the existing `cleanup` path in `main.swift`.
- [ ] **Step 6: Manual check.** Scratch scenario: `play "src-tauri/tests/fixtures/audio/2-speakers-12s.wav" loop` / `wait 15s` / `click "No-such-label"` (forces failure). Run from repo root. Expected: audible playback restarts after 12 s; after the run fails, `pgrep afplay` prints nothing.
- [ ] **Step 7: README** command table rows: `play "file.wav" [loop]` (paths relative to the directory uitest is run from; stops at `stop-audio`, `quit`, or run end) and `stop-audio`.
- [ ] **Step 8: Commit + push**
```bash
git add tools/uitest/Sources tools/uitest/Tests tools/uitest/README.md
git commit -m "feat(uitest): play command for system-audio recording tests" && git push
```

### Task 4: System-audio smoke scenario + agent docs

Claude's call: one committed smoke scenario is the proof that the documented recording pattern works end to end on the real app; it is not an AS11 rewrite.

**Files:**
- Create: `tools/uitest/scenarios/system-audio-smoke.uitest`
- Create: `CLAUDE.md` (repo root)
- Modify: `tools/uitest/README.md` ("Recording tests" subsection)

**Interfaces:**
- Consumes: `play`, `stop-audio` (Task 3); cache (Task 2); signed build (Task 1).

- [ ] **Step 1: Pick the fixture phrase.** Using the signed real build, record `2-speakers-12s.wav` via App audio 3 times (by hand or a draft of the scenario with `labels`). Choose a 2–5 word phrase that appears identically in all 3 transcripts.
- [ ] **Step 2: Write the scenario:** `launch` → `expect-not "Getting Scribular ready" timeout 900s` → `play "src-tauri/tests/fixtures/audio/2-speakers-12s.wav" loop` → `click "Settings"` → `click "App audio"` → `click "Refresh"` (timeout 20s retry if `afplay` absent) → select `afplay` in `App` (use `labels` to find what AX exposes; keyboard selection is fine) → `click "Record" index 1` → `click "Start recording"` → `wait 20s` → `click "Stop"` → `stop-audio` → `expect "<phrase>" timeout 180s` → `expect "Summary ·" timeout 300s` → `quit`. Comment the phrase's origin.
- [ ] **Step 3: Run it twice** from repo root: `swift run --package-path tools/uitest uitest tools/uitest/scenarios/system-audio-smoke.uitest --app <app>`. Expected: both pass; second run's launch-to-ready is under 30 s; no keychain/permission prompts.
- [ ] **Step 4: Negative check.** Temporarily delete the `play` line, run once. Expected: FAIL at the phrase `expect`. Restore the line.
- [ ] **Step 5: Docs.** `CLAUDE.md` "E2E / uitest" section, ≤12 lines: build command from Global Constraints; first-time `setup-signing.sh`; recording tests use App audio + `play`, never the mic; assert fixture phrases, never summary wording; models come from `~/Library/Caches/scribular-uitest/models`, never delete it as a fix; template = `system-audio-smoke.uitest`; details in `tools/uitest/README.md`. README "Recording tests" subsection: the same pattern in steps, pointing at the smoke scenario.
- [ ] **Step 6: Commit + push**
```bash
git add tools/uitest/scenarios/system-audio-smoke.uitest CLAUDE.md tools/uitest/README.md
git commit -m "test(uitest): system-audio smoke scenario; document E2E recording rules" && git push
```
