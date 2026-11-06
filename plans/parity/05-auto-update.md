# Auto-update — Tauri parity spec
Status: draft for cwf; Date: 2026-11-03


> **Dependency policy (Patrick, 2026-11-05; supersedes every dependency-review, Patrick-approval, publish-date cutoff, two-week rule, exact-pin, OSV/RustSec, owner-check or `build.rs`-review requirement in this spec and in `plans/DEPENDENCY_REVIEW_*.md`).** Before adding a crate or npm package, check only that (1) it is commonly used (substantial downloads or dependents) and (2) it has had a release or commit within the last 12 months. Record the result in one or two lines in the PR description. No `DEPENDENCY_REVIEW_*.md` doc and no approval step. Prefer dependencies already in the tree. This covers packages only: a pinned model file still needs its URL, size and sha256 taken from the upstream file, never invented.

## Intent (why users need it; who)

New Tauri users (Mac, Apple Silicon, macOS 14+) install a DMG once. Without in-app update they stay on whatever build they first downloaded and never get fixes. The Electron app checks hourly, downloads in the background and shows a "restart to update" bar. The Tauri app needs the same loop, with two additions Electron lacks: it must never restart the app while the user's work is in flight, and a bad release must be stoppable.

Fresh start (Patrick, 2026-11-03): Electron users stay on Electron. The two apps must never update into each other.

## Electron behaviour today (with file:line refs)

- **Library:** `electron-updater` 6.7.3 (`package.json:34`), required in `src/index.js:33` and `src/main/registerIpcHandlers.js:12`.
- **Only when packaged:** the whole updater block is under `if (app.isPackaged)` (`src/index.js:480`). Feed config is the `app-update.yml` that electron-builder bakes in from `publish` (`src/index.js:479`).
- **Feed:** `electron-builder.json:139-145`: provider `github`, owner `pi0neerpat`, repo `scribular-releases` (public releases-only repo, `docs/PUBLIC_RELEASES_REPO.md:3-10`), `vPrefixedTagName: true`, `releaseType: draft`. Electron publishes `latest*.yml` plus installers there. `build-electron.yml` publishes with `GITHUB_TOKEN: secrets.PERSONAL_ACCESS_TOKEN` (`:338`); the doc names the secret `RELEASES_REPO_TOKEN` (`docs/PUBLIC_RELEASES_REPO.md:40`). The two disagree; the workflow is what runs.
- **Policy:** `autoDownload = true`, `autoInstallOnAppQuit = true` (`src/index.js:481-482`). The first check runs at startup, then every hour (`:530-531`).
- **Events:** `update-downloaded` sends `Channels.APP_UPDATE_READY` (`app:updateReady`, `src/common/ipcChannels.js:63`) to the main window only (`src/index.js:496-505`). Checking, available and not-available are logged only (`:484-494`).
- **Errors:** network errors are matched by message substrings (`net::ERR_`, `ENOTFOUND`, `ECONNREFUSED`, `ECONNRESET`, `ETIMEDOUT`, `ERR_NETWORK`, "Unable to find latest version") and logged at info level. Everything else is logged as an error. The user never sees either (`:507-528`). Brittle: it depends on message text.
- **Install:** invoke `APP_QUIT_AND_INSTALL` (`ipcChannels.js:99`). In production it removes the `window-all-closed` listener and calls `autoUpdater.quitAndInstall(false, true)` on `setImmediate` (`src/main/registerIpcHandlers.js:339-352`). It does **no** busy check. In dev it relaunches. `--e2e-test` makes it a no-op.
- **Relaunch:** `APP_RELAUNCH` is a separate channel for model switch (`registerIpcHandlers.js:354-360`; `plans/MODEL_SELECTION.md:141`). Not an update feature.
- **Preload:** `onUpdateReady`, `quitAndInstall`, `relaunchApp` (`src/preload/preload.js:64,140-142`).
- **UI:** `setupUpdateNotificationListener` (`src/renderer/client.js:412-461`). An in-app bar `#update-notification` has buttons "restart" (`:427-431`) and "dismiss" (`:432-435`, hides until the next show) and a "what's new" link to `https://www.scribular.com/releases` (`:437-442`). The bar is hidden while `recordingStatus.recording` is true and shown again when recording stops if an update is pending (`:449-460`).
- **Versioning:** the `prepare` job bumps the patch in `package.json`, commits to `main`, and tags `vX.Y.Z` (`.github/workflows/build-electron.yml:22-57`). It aborts if a draft release for the tag already exists in the releases repo (`:35-46`).

**Electron gaps the Tauri version must not copy**
1. Only *recording* hides the bar (`client.js:453`). Transcription and summary do not.
2. The hiding is UI-only. `APP_QUIT_AND_INSTALL` has no guard (`registerIpcHandlers.js:339`), and `autoInstallOnAppQuit` (`index.js:482`) installs when the user quits mid-session.
3. Dismiss is not remembered: any later recording stop re-shows the bar (`client.js:456-458`, `updatePending` stays true).
4. No manual "check now", no visible failure, no retry policy beyond the hourly timer.
5. Error classification by message substring.

## Tauri state today (what exists, what is missing, with file refs)

- **Exists:** signed + notarized DMG from `.github/workflows/release-mac.yml` (`workflow_dispatch` only, `macos-latest`, arm64). It sets the six Apple secrets for `npm run tauri build` (`:23-31`), runs verification (`:33-57`), and uploads the DMG as a workflow artifact (`:59-62`). `plans/evidence/release.md` records a passing run (commit `b45a066`, DMG 10.64 MiB) and the dyld incident: a bundled dylib linked an absolute Homebrew OpenSSL path. The workflow now fails if any bundled binary links outside the bundle or the OS (`release-mac.yml:46-50`). New crates must not break that guard.
- **Skipped on purpose in U11:** "Version bumping, publishing to `scribular-releases`" (`plans/2026-11-03-u11-mac-signing-github-actions-plan.md:50-52`).
- **Version today:** `0.1.0` in three places: `src-tauri/tauri.conf.json:4`, `src-tauri/Cargo.toml:3`, `package.json:4`. `commands::app_version` returns `CARGO_PKG_VERSION` (`src-tauri/src/commands.rs:85-90`); `src/App.svelte:17` shows it.
- **Bundle:** `bundle.targets = ["app","dmg"]` (`tauri.conf.json`). No `createUpdaterArtifacts`, no `plugins.updater`, no updater crate. `src-tauri/capabilities/default.json` allows only `core:event:default`.
- **Busy state is available in Rust:** `Recorder::current()` (`src-tauri/src/jobs/session.rs:221`, exposed as `current_recording`, `commands.rs:240`) and `JobQueue::wait_idle(timeout)` (`src-tauri/src/jobs/queue.rs:252-262`; `Duration::ZERO` makes it a non-blocking idle check). `models` downloads are separate (`src-tauri/src/models/download.rs`) and resumable with `Range` (`plans/DEPENDENCY_REVIEW_M3.md`).
- **Missing:** everything else: updater plugin, signing keypair, feed, `latest.json`, version-bump step, publish step, update state machine, UI, Settings row, tests.
- Tauri's MVP plan lists auto-update as out of scope (`plans/TAURI_MVP.md:44`).

## Goals / Non-goals / Anti-goals

**Goals**
- Hourly background check, auto-download, "Restart to update" bar, install on click, install-on-quit only when idle (Electron parity).
- Updates never interrupt recording, transcription, summary or a model download.
- One CI run produces a signed + notarized DMG **and** a signed updater bundle and publishes both with `latest.json`.
- Updates are verified with a Tauri updater key that is separate from the Apple key.
- Electron and Tauri installs can never update into each other.

**Non-goals**
- Intel or Windows updater targets (Mac arm64 only).
- Delta updates, update channels (beta/stable), staged rollout percentages.
- Forced or mandatory updates; the user is never blocked.
- In-app release-notes rendering (link to scribular.com/releases like Electron).

**Anti-goals**
- Do not publish anything Tauri-related to `pi0neerpat/scribular-releases`.
- Do not add a JS updater/process plugin; the Rust core owns update state.
- Do not print, log or invent signing-key material.
- Do not downgrade a client; the updater only accepts a strictly newer version.

## Requirements (numbered R1..)

**Dependencies and keys**
- **R1** First implementation step: check the updater crates (below) against the dependency policy at the top of this spec and note the result in the PR description. No review doc, no approval step.
- **R2** Generate an updater keypair with `tauri signer generate`. It is a minisign-style key, **unrelated to the Apple Developer ID cert**. The public key goes in `tauri.conf.json` at `plugins.updater.pubkey` (public, committed). The private key and its password go **only** in GitHub Actions secrets `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` on `pi0neerpat/scribular-tauri`, set by Patrick with `gh secret set`. The builder never sees, prints or invents key values.
- **R3** Losing the private key strands every installed copy (they can only verify against the baked-in public key). Patrick keeps an offline backup. The runbook states this. There is no in-app key rotation in v1.

**Feed and artifacts**
- **R4** The feed lives in a dedicated **public** repo, default `pi0neerpat/scribular-tauri-releases` (see Decisions). Endpoint: `https://github.com/pi0neerpat/scribular-tauri-releases/releases/latest/download/latest.json`. It is never `scribular-releases`.
- **R5** `bundle.createUpdaterArtifacts = true` so the build emits `Scribular.app.tar.gz` and `Scribular.app.tar.gz.sig` next to the DMG.
- **R6** CI generates `latest.json` with an in-house script (`scripts/make-latest-json.mjs`, Node built-ins only; no `tauri-action`): `version`, `notes`, `pub_date` (RFC 3339), and `platforms["darwin-aarch64"] = { signature: <contents of the .sig file>, url: <release asset URL of the .tar.gz> }`.
- **R7** Publish order: create the release as a **draft**, upload the DMG, the `.tar.gz`, the `.sig`, then `latest.json` last, then flip to published. A half-uploaded release must never be reachable through `/releases/latest/`.
- **R8** CI verifies before publish: (a) the `.sig` verifies against the committed public key; (b) the app inside the `.tar.gz` passes `codesign --verify --deep --strict`, `spctl -a -vv` ("Notarized Developer ID") and `xcrun stapler validate`. If the bundler tars the app before stapling, (b) fails and the workflow must reorder or re-tar and re-sign. Do not assume.
- **R9** The existing Apple secrets and verification steps stay unchanged. The two key systems stay independent: Apple signs the app, the updater key signs the update tarball.

**Version bumping**
- **R10** One source of truth: `src-tauri/Cargo.toml` `version`. Remove `version` from `tauri.conf.json` (Tauri falls back to `Cargo.toml`; confirm with a local `tauri info`/CI build; if not, keep all three in sync) and keep `package.json` in sync. `scripts/bump-version.mjs` (in-house) takes `patch|minor|major`, edits the files and prints the new version.
- **R11** `release-mac.yml` gets `workflow_dispatch` inputs: `bump` (choice, default `patch`, matches Electron `build-electron.yml:22-32`), `publish` (boolean, default `false`), `notes` (string, default `See https://www.scribular.com/releases`). With `publish=false` it behaves as today (artifact upload only), so the current verify run stays cheap.
- **R12** The bump is applied in the runner's working tree before the build. **After** a successful publish, the workflow commits "Bump version to X.Y.Z" to `main` and pushes the tag `vX.Y.Z` (needs `permissions: contents: write`). Doing it after publish avoids orphan bumps and tags from failed builds (Electron commits first, `build-electron.yml:48-57`). The workflow fails early if `vX.Y.Z` already exists or, when the feed has a current `latest.json`, if X.Y.Z is not strictly greater than its version. A missing/404 feed is allowed for the bootstrap release.
- **R13** Release-repo token: a fine-grained PAT with Contents read/write on the releases repo only, in secret `RELEASES_REPO_TOKEN` on `scribular-tauri`. Passed as `GH_TOKEN` env to publish steps only. No `set -x` near it.
- **R14** Guard against feeding Electron: the workflow aborts if the publish target repo is `pi0neerpat/scribular-releases`.

**Runtime**
- **R15** Add `tauri-plugin-updater`, registered in `lib.rs`, configured in `tauri.conf.json` (`pubkey`, `endpoints`). Drive it **from Rust only** (`app.updater()`), through a trait so the state machine is testable with a fake. No JS plugin, no capability additions, no `tauri-plugin-process`. Relaunch with `AppHandle::restart()`.
- **R16** `UpdateManager` (Rust, managed state) owns this state machine: `Idle → Checking → (UpToDate→Idle | Downloading{received,total} → Ready{version,notes} | Error{message} → Checking on the next scheduled or manual retry)`. States are exposed as the `UpdateState` ts-rs type.
- **R17** Schedule: first check ~15 s after launch (off the startup path), then every 60 min (parity with Electron `index.js:530-531`). Skipped in debug builds (parity with `app.isPackaged`, `:480`) unless `SCRIBULAR_UPDATE_ENDPOINT` is set (debug builds only, used for manual tests). A check while one is running or while `Ready` is a no-op.
- **R18** Auto-download after a positive check (parity with `autoDownload`, `index.js:481`). Keep the verified bytes in memory. They are ~11 MB; on quit they are dropped and the next launch re-downloads. The plugin verifies the minisign signature in `download`; a bad signature yields `Error` and the bytes are discarded.
- **R19** **Busy rule.** One shared `busy` definition covers recording, recorder finalization (including the interval after `current()` is cleared and before the summary job is queued, while transcription/diarization still run), queued or running summary jobs, and a running model download. `Recorder::is_busy()` is the single definition used by updates and recording. Checking and downloading are allowed while busy (no user-visible effect). **Installing and restarting are not.**
- **R20** `install_update` acquires the install gate shared with recording start/stop, checks `busy`, and holds the gate through install and restart. A busy result returns `Err("busy")`; recording start/stop cannot cross the check/install/restart interval. The same gate and check guard install-on-quit.
- **R21** Install-on-quit (parity with `autoInstallOnAppQuit`, `index.js:482`): on `RunEvent::ExitRequested`, acquire the shared install gate and, if state is `Ready` and not busy, install the staged bytes, then exit. If busy, exit without installing. An install error here is logged and the exit continues. Quitting is never blocked by the updater.
- **R22** Emit `update://state` with `installable: false` in every state except `Ready` while idle; only `Ready` + not busy emits `installable: true`. While `Ready`, a 2 s ticker re-evaluates `busy` and re-emits only on change, so no hooks into `Recorder` or `JobQueue` are needed. The ticker stops when state leaves `Ready`.
- **R23** Install flow: `install_update` → acquire the shared install gate → check busy → write the pending-install marker (R27) → `update.install(bytes)` → `app.restart()`. On install error: restore state to `Ready` if the bundle is untouched, else `Error`; log; the user keeps a working app (R26).

**UI**
- **R24** `UpdateBar.svelte`, mounted in `src/App.svelte`, visible only when `Ready && installable && !dismissedForThisVersion`: "Scribular X.Y.Z is ready" with **Restart to update**, **Later** and **What's new** (opens `https://www.scribular.com/releases` via the `open_external_link` command that spec 04 introduces; no new plugin). **Later** hides the bar for that version until the next launch (fixes Electron gap 3). While `busy` the bar is hidden, and it reappears the moment `installable` turns true (parity with `client.js:449-460`, now covering transcription and summary).
- **R25** `Settings.svelte` gets an **Updates** section: current version (from `appVersion()`), state text ("Up to date", "Checking…", "Downloading 40%", "Version X.Y.Z ready", "Last check failed: <message>"), a **Check for updates** button (new vs Electron, Spec author's call), and **Restart to update** when `Ready` (disabled with the reason "Finish your recording or summary first" when busy). Network failures during background checks show nothing in the main view; they appear only here.

**Failure handling / rollback**
- **R26** The client never auto-downgrades: only `remote > current` (the plugin's default semver comparison) is accepted. A bad *download* or *signature* is discarded and retried at the next hourly check, with no UI beyond the Settings status. Classify errors by the plugin's error type, not message text.
- **R27** Pending-install marker in the Rust-owned store (settings key-value if the store has one, else a small migration): `{from, to, attempts}`. On launch: running version == `to` → clear it. Running version == `from` (install did not take) → `attempts += 1`. After 3 failed attempts for the same `to`, record `skipped_version = to` and stop auto-downloading it until a newer version appears. Log each outcome.
- **R28** Release-side rollback runbook in `plans/RELEASING.md`: a bad release is pulled by switching it back to **draft** in the releases repo. `/releases/latest/download/latest.json` then resolves to the previous release, so machines that have not updated stay put. Machines already on the bad build do **not** downgrade (R26); fix forward with a higher version. Never delete the private key to "stop" updates.
- **R29** Data safety: update installs replace only the `.app` bundle. User data in `app_data_dir` is untouched. Forward-only SQLite migrations mean a rolled-back binary may meet a newer DB. See Interacts: the release runbook warns about it and every release with a migration notes it.

## Acceptance scenarios (numbered AS1..; Given/When/Then; each tagged automated (cargo test / fake-engines / vitest) or manual real-app check)

**AS1 (automated, cargo test + fake updater)** Given a fake updater reporting 0.2.0 while the app is 0.1.0 and idle. When the check runs. Then state goes `Checking → Downloading → Ready{0.2.0}` and `update://state` is emitted with `installable: false` for `Checking` and `Downloading`, and `installable: true` only for idle `Ready`.

**AS2 (automated)** Given the fake reports the same or an older version. When the check runs. Then state is `Idle` (up to date), no download starts.

**AS3 (automated)** Given state `Ready` and `Recorder::current()` is `Some` or the recorder is `Finalizing`. When `install_update` is invoked. Then it returns `Err("busy")`, the fake `install` was not called, and `restart` was not called. Repeat with a queued summary job, a running job, and a running model download. While installation holds the shared gate, recording start and stop cannot cross the install/restart interval.

**AS4 (automated)** Given `Ready` while recording. When recording stops and the queue drains. Then within one ticker period the manager emits `installable: true` exactly once, and not before.

**AS5 (automated)** Given `Ready` and idle. When `install_update` is invoked. Then the shared gate is acquired, the marker `{from:0.1.0,to:0.2.0,attempts:0}` is written before `install`, `install` is called once, then `restart` while the gate remains held.

**AS6 (automated)** Given the exit event while `Ready` and idle. Then `install` is called once before exit. Given the same while busy: `install` is not called, exit proceeds. Given `install` fails on quit: exit still proceeds.

**AS7 (automated)** Given the fake `download` returns a signature error. Then state is `Error`, no bytes are kept, the next scheduled check retries, and no `Ready` is ever emitted.

**AS8 (automated)** Given the fake `install` fails. Then the user is returned to a usable state per R23 and `restart` is not called.

**AS9 (automated)** Given the marker `{from:0.1.0,to:0.2.0,attempts:2}` and the app launches as 0.1.0. Then `attempts=3`, `skipped_version=0.2.0`, and a later check offering 0.2.0 does not download; a check offering 0.2.1 does. Given launch as 0.2.0, the marker is cleared.

**AS10 (automated, cargo test reading config/source as `lib.rs` tests do)** `tauri.conf.json` has a non-empty `plugins.updater.pubkey`, `createUpdaterArtifacts: true`, and every endpoint is `https://` and does not contain `scribular-releases`. `make-latest-json.mjs` output (node test or a `cargo test` that runs it on fixture input) has the three required fields and `platforms["darwin-aarch64"].{signature,url}`. `bump-version.mjs` bumps patch/minor/major in all files consistently (fixture dir).

**AS11 (automated, vitest)** `UpdateBar` renders only for `ready && installable`. **Later** hides it until a new version arrives. Busy → hidden, and it shows once `installable` flips. Settings shows each state's text and disables the restart button with the reason when busy. `api.ts` wrappers (`checkForUpdates`, `installUpdate`, `getUpdateState`, `onUpdateState`) invoke the right command names; the existing command/api name-parity test in `lib.rs` passes.

**AS12 (manual, CI)** Run `release-mac.yml` with `publish=false`. The run passes with the new signing env, produces `.tar.gz` and `.sig`, and the R8 checks pass, including `stapler validate` on the app extracted from the tarball. Record the run URL in `plans/evidence/update.md`.

**AS13 (manual, real app, full loop)** With `publish=true` ship v0.1.1 over an installed v0.1.0 (the first updater-enabled build). On a clean second Mac (or fresh user), launch 0.1.0, wait for the check (or press **Check for updates**), see the bar, click **Restart to update**. Then the app relaunches as 0.1.1, `spctl` still says "Notarized Developer ID", Gatekeeper shows no prompt, and existing sessions are still listed. Use `dwarfdump --uuid` to confirm the binary changed (`release.md` lesson).

**AS14 (manual, real app)** Start a recording with an update `Ready`. The bar is not shown. Settings shows restart disabled with the reason. Press stop; while the summary runs, the bar is still hidden; when it finishes, the bar appears. Quit during a recording: no install happens; the next launch re-downloads and offers the update.

**AS15 (manual)** Fresh-start isolation. An Electron 1.5.10 install, left running for an hour on a network that reaches the feed, is never offered a Tauri build (no `scribular-releases` release exists for it). A Tauri build pointed at the feed rejects an artifact whose `.sig` was made with a different key (corrupt a byte of the tarball in a draft staging release; expect `Error`, no restart).

**AS16 (manual)** Offline launch: no UI error, a log line, and Settings shows "Last check failed". After reconnecting, **Check for updates** works.

**AS17 (manual, runbook)** Switch the published release to draft: `latest.json` resolves to the previous version and a 0.1.0 client sees no update to the pulled version.

## Design notes (modules, commands/events, data model + migration, UI components)

**Rust**
- `src-tauri/src/update/mod.rs` (new): `trait UpdateSource { check() -> Result<Option<UpdateInfo>>; download(&UpdateInfo, on_progress) -> Result<Vec<u8>>; install(&UpdateInfo, Vec<u8>) -> Result<()> }`. Real impl `TauriUpdateSource` wraps `app.updater()`. Fake impl in tests (same pattern as the fake engines). `UpdateManager { state, staged: Option<(UpdateInfo, Vec<u8>)>, install_gate, busy }`, with `update/state.rs` for transitions, mirroring `jobs/state.rs`.
- `src-tauri/src/jobs/session.rs`: make `Recorder::is_busy()` the shared definition, including active recording, recorder finalization, queued/running summary work and model download. Recording start/stop and update installation acquire the same install gate; installation holds it through restart.
- `src-tauri/src/commands.rs`: `check_for_updates()`, `get_update_state()`, `install_update()`. Add them to `generate_handler!` in `lib.rs:49-69` (the parity test in `lib.rs` will force the matching `api.ts` wrappers).
- `src-tauri/src/lib.rs`: `.plugin(tauri_plugin_updater::Builder::new().build())`, `app.manage(UpdateManager)`, spawn the scheduler thread in `.setup`, handle `RunEvent::ExitRequested` for R21 (needs `.build(...).run(|app, event| ...)` if the app currently uses `.run(generate_context!())`).
- Event: `update://state` carrying `UpdateState` (ts-rs generates `src/lib/types/UpdateState.ts`).
- Store: the pending-install marker and `skipped_version` (R27), via the existing settings mechanism in `src-tauri/src/store/`, or a migration `NNN_update_state` if that is cleaner. The builder checks `store/` and picks.
- `src-tauri/tauri.conf.json`: `bundle.createUpdaterArtifacts: true`; `plugins.updater: { pubkey, endpoints: [...] }`; drop `version`.
- `src-tauri/capabilities/default.json`: unchanged (Rust-driven).

**Frontend**
- `src/lib/api.ts`: `checkForUpdates`, `getUpdateState`, `installUpdate`, `onUpdateState`.
- `src/lib/components/UpdateBar.svelte` (new), mounted in `src/App.svelte`; update section in `src/lib/components/Settings.svelte`; a pure `update-status.ts` for the state → text mapping (same style as `record-status.ts`) so vitest covers it without rendering.

**CI** (`.github/workflows/release-mac.yml`)
1. checkout, setup, `npm ci` as today.
2. `bump-version.mjs ${{ inputs.bump }}`; export `VERSION`.
3. Build step: existing Apple env plus `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (secrets only).
4. Existing verification; add the tarball extraction + R8 checks.
5. `make-latest-json.mjs`.
6. If `inputs.publish`: guard R14, `gh release create --draft` on the releases repo with `GH_TOKEN=RELEASES_REPO_TOKEN`, upload assets, `latest.json` last, `gh release edit --draft=false`, commit bump + tag.
7. Always upload the DMG artifact as now.

**First-release baseline.** 0.1.0 shipped to testers from the current workflow has no updater key or endpoint and cannot self-update. The first updater-enabled build is the baseline; earlier testers reinstall once by hand.

**Name collision to watch.** Electron's mac bundle is also "Scribular" (`appId com.app.scribular`, `electron-builder.json:2`), the Tauri bundle id is `com.app.scribular.tauri` (`tauri.conf.json`). Both would install to `/Applications/Scribular.app`. Each updater replaces the bundle it is running from, and the feeds differ, so there is no cross-update, but a user dragging both DMGs overwrites one with the other. Owned here as a decision (Decision 5); this spec does not change the name unless Patrick chooses to.

## New dependencies (candidates + review needed) or "none"

Check (R1): per the dependency policy at the top of this spec, each candidate is commonly used and had a release within the last 12 months. No review doc.

| Candidate | Why | In-house alternative |
|---|---|---|
| `tauri-plugin-updater` (Rust only) | Check, download, minisign verify, tarball extract, atomic `.app` replace. | `reqwest` (present) + `minisign-verify` + `tar`/`flate2` + own bundle replace. That is still three new crates plus security-critical file replacement. The plugin wins. |

Review points:
- **TLS features.** Check its default features. Avoid any default that pulls rustls with `aws-lc-rs` (CMake/C build, see M3 notes). Prefer the `native-tls` feature to match `reqwest` in `Cargo.toml` and macOS system trust. Confirm `otool -L` still passes the bundle guard (`release-mac.yml:46-50`).
- **Not added:** `@tauri-apps/plugin-updater` (JS), `tauri-plugin-process`, `tauri-action`. `AppHandle::restart()` and `gh` + a small Node script replace them.
- **Tooling, not a dependency:** `tauri signer generate` (from the existing `@tauri-apps/cli`).

## Out of scope

- Intel/Windows/Linux updates; beta channels; staged rollout; delta updates.
- Key rotation tooling (R3 documents the risk).
- Rendering release notes in-app.
- The `download.scribular.com` / Pecans download server and the website releases page content.
- Migrating or updating Electron installs; changing the Electron app or feed.
- Making the Tauri bundle name distinct from the Electron one, unless Patrick picks that in Decision 5 (then it is a `productName` change in this spec).

## Depends on / interacts with (other parity specs by slug)

- **01-encryption-audio-retention:** release runbook (R29) must note any release that migrates the SQLite/SQLCipher schema; rollback is forward-only. Keychain-held keys survive a bundle replacement only if the code signature/identity stays the same; the same Developer ID is used, so verify in AS13.
- **04-licensing-trial:** licence checks are not network-gated by the updater; the update feed must stay reachable without a licence. No dependency otherwise.
- **06-diagnostics-telemetry:** update outcomes (check failed, signature failed, install failed, attempts) go through the logging that spec defines. Until then `eprintln!` like the existing code.
- **08-app-shell-tray-notifications:** if a tray menu exists, it may later show "Update ready". Not required here. Quit paths (menu, Cmd-Q, tray) must all reach `RunEvent::ExitRequested` so R21 behaves the same.
- **09-session-management-recovery:** killing the app mid-install is not recoverable by this spec; crash recovery of interrupted recordings is theirs. Our busy rule exists to avoid the situation.
- **03-note-styles-model-choice:** 03 R20 switches models without a relaunch, so no relaunch path is needed. `install_update` stays update-only.

## Decisions for Patrick (with recommended default)

1. **Where does the feed live?** Default: a new public repo `pi0neerpat/scribular-tauri-releases`. Why: `scribular-releases` is read by the 1.5.x Electron updater, and I could not verify from source how electron-updater's GitHub provider treats extra or prerelease tags, so sharing it risks Electron users seeing Tauri releases. `scribular-tauri` itself is not known to be public; the updater needs unauthenticated downloads. If `scribular-tauri` is or will be public, publishing there is a valid alternative with one fewer repo.
2. **Create the signing keypair and set secrets** (`TAURI_SIGNING_PRIVATE_KEY`, `..._PASSWORD`, `RELEASES_REPO_TOKEN`) with `gh secret set -R pi0neerpat/scribular-tauri`, and back the private key up offline. Default: Patrick does this; the builder prepares everything else and writes the exact commands in `plans/RELEASING.md` without values.
3. **Install-on-quit when idle** (Electron parity) or click-only? Default: include (R21), since users who ignore the bar would otherwise stay outdated.
4. **Release gate:** should a published release require a manual approval (GitHub environment) before the draft goes public? Default: no; `publish=false` dry-run plus manual dispatch is the gate.
5. **App identity vs the Electron app (added by gap-check 2026-11-04).** Both apps install as `/Applications/Scribular.app` (Electron `productName Scribular`, `appId com.app.scribular`; Tauri `tauri.conf.json:3,5`). (a) Bundle identifier: default **keep `com.app.scribular.tauri` and freeze it**. It is the Keychain service for 01's DB key and 04's licence item and names the data dir, so changing it after testers have data orphans their key (Locked screen). (b) Install name: default **keep "Scribular.app"** and tell existing Electron users in release notes that the new app replaces the old one in /Applications (data is separate: Electron data stays in its own folder and is not migrated). Alternative: a distinct `productName` (e.g. "Scribular (Beta)") during the overlap period. Only (a) affects spec 01, and the default keeps today's value, so this does not block 01.

## Coordination notes (gap-check 2026-11-04)
- **Release/bundle owner:** this spec owns the release workflow and app identity (Decision 5). Native-lib additions in other specs extend the same verify step (`release-mac.yml:33-57`): 01 adds a no-`libcrypto`/`libssl` check, 02 adds ONNX/sherpa dylibs.
- **Links:** use spec 04's `open_external_link`.
- **Pending-install marker (R27):** store it in the settings KV table (no migration). After 01, settings are inside the encrypted DB, so read it after unlock; if the store is Locked, skip the post-update check rather than fail.
- **Test mode:** Electron makes `quitAndInstall` a no-op under `--e2e-test` at runtime. Do not copy this. Any test-only behaviour is compile-time (`cfg(test)`/feature).

## Size (S/M/L, rough days) and suggested PR split if > ~1 PR

**M, about 5 days.**
1. PR0: dependency policy check (under 0.5 day; fold into PR1 if trivial).
2. PR1 (2 days): `update/` module, trait + fake, state machine, `Recorder::is_busy`, commands, events, marker logic, AS1-AS9.
3. PR2 (1 day): `api.ts`, `UpdateBar`, Settings section, vitest AS11.
4. PR3 (1.5 days): keypair handoff, `tauri.conf.json`, bump + latest.json scripts, workflow changes, `plans/RELEASING.md`, AS10/AS12. Then manual AS13-AS17 with real v0.1.1.
