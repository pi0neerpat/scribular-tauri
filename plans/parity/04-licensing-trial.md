# Licensing and free trial — Tauri parity spec
Status: draft for cwf; Date: 2026-11-03

## Intent (why users need it; who)
Scribular is sold as a one-time (lifetime) licence with a 14-day free trial. The Tauri app is a fresh-start product for new users, so it needs its own trial clock, licence-key entry, and a gate on *starting new recordings* once the trial or licence lapses. Without it the Tauri build cannot be sold. Users are therapists and other privacy-sensitive professionals. Two promises constrain the design: the app stays offline after the model download, and a lapsed licence never locks anyone out of notes they already have.

## Electron behaviour today (with file:line refs)
Paths are relative to `notes-assistant-electron/`.

**Trial**
- Trial length is 14 days (`src/utils/licenseService.js:7`).
- `firstLaunchDate` is written once, as an ISO string, in the electron-store config at first launch (`src/store/store.js:70-72`). It is plain JSON on disk, so deleting the config resets the trial. No server records it.
- Days elapsed use `Math.floor(ms/day)`. The trial has ended when elapsed days ≥ 14 (`licenseService.js:107-123`). A missing date counts as ended (`:108`).
- The trial warning appears when ≤ 3 days remain (`:246-251`).

**Licence and backend**
- A device ID is a random UUID created on first init and kept in the config (`src/store/store.js:15-22`, `:74`).
- `validateLicense(licenseKey, deviceId)` POSTs `{licenseKey, deviceId}` as JSON to `${apiRoot}/api/validate` (`licenseService.js:40-46`).
- The API root is `https://www.scribular.com`, or `http://localhost:3000` in debug builds (`src/utils/apiConfig.js:6-10`).
- Any non-2xx response or fetch error becomes `{valid:false, networkError:true, error:"Unable to validate license..."}` (`:48-59`). A 500 from the server therefore reads as "network error".
- Backend, read-only: `scribular-saas-project/app/api/validate/route.ts`.
  - The route checks the key format `word-word-word-123` (`lib/license.ts:19-42`), then that the device ID is a UUID.
  - It looks the key up in the `licenses` table (`prisma/schema.prisma:13-31`).
  - If `stripeCurrentPeriodEnd < now` it returns `valid:false, "License has expired"` (`route.ts:54`).
  - A known device returns `valid:true, expirationDate` (`:61-68`).
  - A new device is registered into `license_devices` if fewer than 2 exist (`MAX_DEVICES_PER_LICENSE = 2`, `:8`, `:70-83`). Otherwise it returns "License limit reached" (`:70-76`).
  - Licences are issued after Stripe checkout with `stripeCurrentPeriodEnd = 2099-12-31` (`lib/fulfill-license.ts:9`, `:90`). In practice they are lifetime licences.
  - Responses are `{valid, expirationDate}` or `{valid:false, message}`. **Note:** the Electron client reads `result.error`, not `result.message` (`src/renderer/views/settings.js:967`). The server's useful reasons ("limit reached", "not found") are therefore never shown. The generic text is shown instead.
  - There is a hard-coded test key, `testing-success-shoemaker-392`, that returns valid in production (`route.ts:14`, `:20-25`). **This is a backdoor.** Do not copy it into the Tauri client. Flag it to Patrick (see Decisions).
- The key, `licenseValid` and `licenseExpirationDate` are stored in the same plaintext electron-store config (`store.js:103-107`, `:127-132`, `:149-154`). The key is not encrypted or placed in the Keychain.
- Dev and e2e bypasses exist: env `SCRIBULAR_TEST_LICENSE`, `SCRIBULAR_SKIP_LICENSE`, `--e2e-test` (`licenseService.js:22-30`, `:192-199`).

**Check cadence**
- `checkAndUpdateLicense` (`src/index.js:91-142`) runs on a 1-hour interval (`ONE_HOUR`, `:145`). Only the constant is confirmed here; I did not trace the `setInterval` call or any startup call.
- If the response is a network error, the cache is left alone (`:97-111`). If the server answered, `licenseValid` and the expiry are overwritten (`:98-103`).
- **Offline grace today:** the cached `licenseValid:true` is trusted indefinitely, until the cached `expirationDate` passes (`licenseService.js:206-227`). Because that date is 2099, offline use is effectively unlimited. There is no "last verified" timestamp and no maximum offline period.
- **Revocation:** if the server answers `valid:false` (key removed, or expired), the cache flips to invalid, and `checkAccess` returns "Your license key is invalid" (`:231-236`). Any non-network answer therefore revokes.

**Access decision (`checkAccess`, `licenseService.js:181-255`)**
1. A valid, non-expired licence gives access. A licence within 7 days of expiry sets a warning (`:214-218`).
2. A key with `licenseValid === false` gives no access (`:231-236`).
3. No key: access until the trial ends (`:239-252`).

**Enforcement**
- Main process:
  - `AUDIO_START_RECORDING` and `AUDIO_START_CAPTURE` call `checkAccess`. If denied they send `ACCESS_DENIED {reason}` and return (`src/main/registerIpcHandlers.js:57-77`).
  - **`ACCESS_DENIED` has no listener anywhere in the renderer** (grep finds only the channel definition and the two senders). The user gets no feedback from this path. It is dead feedback, though the block itself works.
- Renderer: `RESTRICT_APP_ACCESS {restrict, message}` is sent by `restrictAppAccess` (`src/index.js:71-88`). `access.js:12-52` handles it:
  - It disables the start button, system-audio button and both source toggles.
  - It sets the status text to "Please purchase a license to continue using Scribular.".
  - It adds an `attention` class to the settings icon.
  - `isRestricted()` guards recording start at `src/renderer/views/recording.js:331` and `:571`.
- **Bug:** a *warning* (≤ 3 trial days left, or ≤ 7 licence days left) calls `restrictAppAccess(..., true, warningMessage)` (`src/index.js:119-121`, `:450-456`). That disables recording during the warning window, although `hasAccess` is still true. Do not replicate this. A warning must not restrict.
- `LICENSE_REQUEST_STATUS` is renderer → main (`src/preload/preload.js:120`, `src/index.js:440-467`). It re-evaluates access and shows a native dialog. It is called after a successful save (`settings.js:963`) and at startup in `client.js`; I did not confirm the startup call.
- `showLicenseDialogs` (`src/index.js:152-204`) is a native dialog, throttled to once per 5 seconds.
  - Title "Access Restricted", with buttons "Okay" and "Purchase License".
  - The purchase button opens `https://www.scribular.com/#pricing` externally.
  - **Bug:** `dialog.showMessageBox` is used without `await`, so `choice === 1` compares a Promise to 1 and never matches (`:174-189`). The purchase button therefore does nothing.
- **Existing notes are not gated.** Only recording-start controls are disabled. Session history, viewing, copy, regenerate and settings stay available. The spec keeps this and makes it a hard requirement.

**UI**
- Settings has a "License" panel (`src/renderer/views/settings.js:466-526`, `:935-1022`, styles in `src/renderer/ui/license.css`):
  - With no key: a key input, a Save button and a "Free trial: N days remaining" line. When the trial is over the line reads "Your free trial has ended. Please purchase a license to continue." (`:495-521`).
  - With a key: a Valid/Invalid badge and the masked key (`first4 + *** + last4`, `licenseUtils.js:26-40`) with a Remove button. Remove asks for confirmation, then clears the key, validity and expiry (`:985-1020`).
- A sidebar badge shows "Licensed", "Trial (N days remaining)", "License expired" or "No license" and links to the panel (`:530-561`).
- `LICENSE_SAVE` (`registerIpcHandlers.js:271-286`) validates and persists the key **only if valid**. It returns `{success, ...result}`. On failure the client clears the input and shows `result.error` or a generic message (`settings.js:965-970`).

**Data that leaves the device (licence path)**
- Licence key and device UUID, to `/api/validate`, at save time and about hourly. The server learns the key, the device ID, the IP address and timing. No audio, notes or transcripts are sent.
- The logger writes the masked key and the full device ID (`licenseService.js:32`).
- Out of scope, for contrast: `usageAnalytics.sendUsage(deviceId)` POSTs weekly stats to `/api/usage` (`src/utils/usageAnalytics.js:180-205`; backend `app/api/usage/route.ts`). The same device UUID links usage rows to the licence's device row on the server. That linkage matters for the privacy stance (see Design notes). The telemetry spec owns the analytics itself.

## Tauri state today (what exists, what is missing, with file refs)
Paths are relative to `scribular-tauri/`.
- No licence or trial code exists. `plans/TAURI_MVP.md:44` and the prototype plan (`plans/2026-10-19-2001-feat-tauri-rewrite-evidence-prototype-plan.md:131`) list licence/trial as out of scope.
- The UI is Svelte 5: `src/App.svelte`, with `RecordView.svelte`, `Settings.svelte` and `FirstRun.svelte` in `src/lib/components/`. There is no licence panel.
- The IPC surface is `src-tauri/src/commands.rs`, with wrappers in `src/lib/api.ts` and ts-rs types.
- Store: rusqlite `bundled`, with ordered migrations in `src-tauri/src/store/migrations.rs` (currently v1 to v3, tracked via `user_version`, `:11`). A key-value `settings` table exists (`migrations.rs:82`, `store/settings.rs:43-47`). The store is **not encrypted** (prototype KTD1, plan `:164`). `keyring` is dropped; encryption is a separate spec (01).
- CSP is `default-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:` (`src-tauri/tauri.conf.json:21`). The webview therefore cannot call a remote API. **All network access must go through Rust.** This fits the privacy promise.
- `reqwest 0.13.5` with `native-tls` is already a dependency (`src-tauri/Cargo.toml:29`, for model download, DEPENDENCY_REVIEW_M3). It can be reused.
- Recording starts through `start_recording` in `commands.rs:199`, and the `Recorder` lives in `src-tauri/src/audio`. This is the choke point for the gate.

## Goals / Non-goals / Anti-goals
**Goals**
- A 14-day free trial that starts at first launch.
- Licence-key entry and validation against the existing `scribular.com/api/validate`, unchanged.
- A Rust-side gate on starting new recordings (and transcribing imported files; see R9).
- Clear status in the UI: trial days left, licensed, lapsed.
- The licence key stored in the macOS Keychain.
- Honest offline behaviour, with a bounded grace period.

**Non-goals**
- Migrating Electron licences or trial dates (fresh start).
- Server changes, new endpoints or accounts. The existing API is reused as-is.
- In-app purchase. The app opens the pricing page in the browser.
- Anti-tamper or anti-piracy hardening beyond casual reset resistance.

**Anti-goals**
- Never block viewing, searching, copying, exporting, deleting or regenerating existing notes or sessions.
- Never block settings, model management, update installation or licence entry.
- Never send audio, transcripts, notes or note metadata.
- Never call the network from the webview, and never contact the network at startup in a way that blocks the UI.
- Do not copy the test-key backdoor or the Electron warning-restricts bug.

## Requirements (numbered R1..)
R1. Add a Rust `license` module. It owns the trial clock, the licence record, and the pure function `access_status(now, state) -> AccessStatus`. The webview never decides access.

R2. **Trial clock.** Store `trial_started_at` (UTC, RFC 3339) once on first launch. Never overwrite it. The trial is 14 days: ended when `floor((now − start)/1 day) ≥ 14`, matching Electron. If the system clock is earlier than `trial_started_at` (clock rolled back), treat elapsed time as `max(0, now − start)` and also keep a monotonic high-water mark (`last_seen_at`, updated at each launch and hourly). Use `max(now, last_seen_at)` for elapsed time. This defeats the trivial clock-back trick. It does not defeat deleting app data. Accepted (see Decisions).

R3. **Trial persistence.** Store the trial start in the SQLite `settings` table (no migration; `settings` is a key-value table and keys are inserted on first launch). Also write `trial_started_at` into the single R10 Keychain item (account `license`) so an app-data wipe does not silently restart the trial.

R4. **Device ID.** On first launch generate a random UUIDv4, store it in the Keychain next to the licence (R10) and mirror it in `settings`. It is the only identifier sent, and only to the licence API. Do not reuse it for analytics (see Decisions and spec 06).

R5. **Validate command.** `activate_license(key: String) -> ActivationResult`.
- Trim the key and check the format `^[a-z]+-[a-z]+-[a-z]+-\d{3}$` case-insensitively, locally, before any network call. Match `lib/license.ts` exactly; the builder must read that regex at `scribular-saas-project/lib/license.ts:40-42`.
- POST `{licenseKey, deviceId}` to `https://www.scribular.com/api/validate` using reqwest, with timeouts (connect 5 s, total 15 s) and no redirect following to other hosts.
- Parse the server `message` (the Electron client ignored it) and map it to the typed result enum `Valid{expires_at}`, `InvalidKey`, `Expired`, `DeviceLimit`, `BadFormat`, `Offline`, `ServerError`.
- Persist the key and `last_verified_at` only on `Valid`.
- On HTTP ≥ 500 or a transport error return `Offline` or `ServerError`. Never treat these as a revocation.

R6. **Re-validation.** Validate silently on launch (non-blocking, after the UI is up) and then at most every 24 h while running. Electron does it hourly (`index.js:145`). The licence key and device ID leave the device on every check, and hourly adds traffic without adding safety for a lifetime licence. Spec author's call: 24 h, to send less.
- A definitive server answer `valid:false` marks the licence **invalid** (revoked or expired). Clear `last_verified_at`.
- Network or 5xx failures change nothing.

R7. **Offline grace.** A licence last verified valid within the grace window stays valid offline. Grace is **30 days** from `last_verified_at`. After 30 days with no successful verification the app shows a non-blocking banner, "Connect to the internet to re-verify your licence", and (R8) restricts new recordings only after a further **7-day** soft period. In total: 37 days of offline use after the last good check, and users who never go online for 37 days are the edge case. Also keep Electron's `expirationDate` check: a verified expiry date in the past is lapsed. The first activation needs a connection; the app says so plainly.

R8. **Access states** (enum `AccessState`, ts-rs exported):
- `Trial{days_left}`
- `TrialWarning{days_left}` (≤ 3 days)
- `TrialEnded`
- `Licensed{expires_at, verified_at}`
- `LicensedGrace{days_left}` (needs re-verification soon)
- `LicenseInvalid{reason}`
- `LicenseLapsed`
Only `TrialEnded`, `LicenseInvalid` and `LicenseLapsed` deny access. Warnings never deny.

R9. **What a lapsed state restricts.** Exactly these actions, enforced in Rust:
- `start_recording` and anything else that creates a new session from live audio.
- `transcribe_file` (`commands.rs:246`, import of new audio).
They return a typed `AccessDenied { state }` error, which the UI turns into the lapsed prompt. Everything else works: list, open, search, edit, copy, delete, regenerate summaries for **existing** sessions, settings, model download, licence entry. A recording already in progress when access lapses is allowed to finish and is saved, and summarisation completes. Never discard or interrupt a session mid-recording.

R10. **Secure storage.** Store `{license_key, device_id, trial_started_at}` as a single Keychain generic-password item under service = the bundle identifier read from config at runtime (`com.app.scribular.tauri` today, `tauri.conf.json:5`; never hard-code), account `license`, using spec 01's `KeyStore`/`KeychainKeyStore` (account-parameterized). This is the only Keychain item this spec adds (R3 and R4 refer to it).
- Store `last_verified_at` and `expires_at` in `settings` (non-secret).
- The Keychain item is "this device only" (`kSecAttrAccessibleWhenUnlockedThisDeviceOnly` semantics) and does not sync to iCloud.
- The key never appears in logs. Log only the masked form (first 4, last 4).
- If the Keychain is unavailable or denied, fall back to the `settings` table, log a warning, and show nothing alarming to the user. (After spec 01, a denied Keychain already puts the app in the Locked state before licence code runs, so this fallback only covers a failure on the `license` item alone; the `settings` table is SQLCipher-encrypted.)
- Do not store the key in plaintext files.

R11. **UI.**
- A "License" section in `Settings.svelte` with the same states as Electron: a key input and Save; the trial line; a "Valid"/"Invalid" badge with the masked key and a confirmed Remove.
- A sidebar or header badge: "Licensed", "Trial (N days remaining)", "Trial ended".
- A `LicenseGate.svelte` panel in `RecordView.svelte`: when denied, the start control is disabled and a message explains why, with two buttons, **Enter licence** and **Buy licence**. The Buy button opens `https://www.scribular.com/#pricing` in the default browser through a Rust `open_external_link(url)` command introduced here in the shape spec 08 R22 defines (parse with `reqwest::Url`, allow-list `http`/`https`/`mailto`, launch via macOS `open` behind an injectable launcher for tests; no new plugin). Specs 05-08 reuse it; 08 adds the navigation guard and global link handler.
- The trial banner (≤ 3 days) is dismissible per day and never blocks the start button.
- The gate message must state that existing notes remain available.
- All strings are plain, and each error is specific (R5 enum), unlike Electron's single generic message.

R12. **Events.** Emit `license://changed` with the new `AccessState` after activation, removal, re-validation and day rollover. The UI subscribes, so no polling.

R13. **Remove licence.** `deactivate_license` clears the Keychain key and `settings` verification fields. It makes **no** network call. The server keeps the device slot; document that support can free slots. Do not call a server deactivate endpoint, because none exists.

R14. **Tests and dev bypass.** There is no production backdoor key. Under the `fake-engines` feature (or `cfg(test)`), the license client sits behind a trait `LicenseApi`, with a fake. For real-app verification of the paid path without a purchase, document that Patrick supplies a real key from the Stripe/admin side. A build-time dev override (`SCRIBULAR_SKIP_LICENSE`) exists only in debug builds (`cfg(debug_assertions)`) and is compiled out of release builds.

R15. **Privacy statement.** Add a short, accurate "What is sent" note under the licence input: "Your licence key and a random device ID are sent to scribular.com to confirm your licence. Your recordings and notes never leave this Mac." Add the same facts to the privacy copy owned by spec 06 and 07.

## Acceptance scenarios (numbered AS1..; Given/When/Then; each tagged automated or manual)
AS1. [automated: cargo test] Given a fresh store, when `license::init` runs twice, then `trial_started_at` is written once and unchanged; the device ID is a valid UUIDv4 and stable.

AS2. [automated] Given `trial_started_at` = now − 13d 23h, when `access_status` is called, then state is `TrialWarning{days_left:1}` and access is allowed. Given now − 14d, then `TrialEnded` and access is denied. (Table-driven, injected clock.)

AS3. [automated] Given `last_seen_at` is later than the system `now` (clock set back 10 days), when `access_status` runs, then elapsed time uses `last_seen_at`, so the trial does not extend.

AS4. [automated: fake LicenseApi] Given a well-formed key and the server returns `{valid:true, expirationDate:"2099-12-31"}`, when `activate_license` runs, then the result is `Valid`; the key is in the Keychain fake; `last_verified_at` is set; state is `Licensed`; `license://changed` is emitted.

AS5. [automated] Given the server returns `valid:false` with message "License limit reached…", then the result is `DeviceLimit`, nothing is persisted, and the existing state is unchanged. Repeat for "License key not found" (`InvalidKey`), "License has expired" (`Expired`), and "Invalid license key format" (`BadFormat`).

AS6. [automated] Given a malformed key such as `abc`, when `activate_license` runs, then it returns `BadFormat` and the fake API records **zero** calls.

AS7. [automated] Given a licensed state verified 10 days ago and the API unreachable, when re-validation runs, then the state is still `Licensed`; access is allowed; nothing is cleared.

AS8. [automated] Given verified 31 days ago and offline, then `LicensedGrace` with a banner and access allowed. Given verified 38 days ago and offline, then `LicenseLapsed` and new recordings are denied.

AS9. [automated] Given a licensed state and the server answers `valid:false`, when re-validation runs, then the state is `LicenseInvalid` and new recordings are denied. Given a 503 or timeout instead, then nothing changes.

AS10. [automated] Given `TrialEnded`, when `start_recording` and `transcribe_file` are called, then both return `AccessDenied`; while `list_sessions`, `get_session`, `update_session`, `delete_session`, `regenerate_summary` and `set_recording_settings` all succeed.

AS11. [automated] Given a recording in progress and the state flips to lapsed mid-recording, when `stop_recording` runs, then the session is saved with its transcript and the summary job completes.

AS12. [automated] Given any state, then the log output from activation contains the masked key and never the full key (assert on captured log text).

AS13. [automated: vitest] The `LicenseGate` component renders the denied state with Enter and Buy buttons and the "existing notes are still available" text. The Buy button calls the opener with `https://www.scribular.com/#pricing`. A warning state renders the banner and leaves the start button enabled.

AS14. [manual real-app] On a fresh macOS user account, launch the release build. Settings shows "Trial (14 days remaining)". The Keychain Access app shows an item `<bundle identifier> / license` (`com.app.scribular.tauri / license` today).

AS15. [manual real-app] On a throwaway macOS user, move the system clock forward 15 days (the DB is SQLCipher-encrypted after spec 01, so hand-editing `trial_started_at` is not practical; a debug-build-only injected clock override is the alternative). Relaunch. The start button is disabled with the lapsed message. Past sessions open, copy and regenerate normally (before spec 09 there is no history UI: check the last session's result view plus `list_sessions`/`get_session` and `regenerate_summary` succeed while lapsed; re-check from History in 09). Enter a real key, and recording works immediately without a restart.

AS16. [manual real-app] Disconnect from the network. Entering a key shows "Couldn't reach scribular.com" and does not accept the key. Reconnect and it succeeds. A licensed install then used offline for a week records normally.

AS17. [manual real-app] Network capture (e.g. Little Snitch or `nettop`) over a recording session plus summarisation shows no connection to scribular.com except `/api/validate` on launch and on activation. No body contains anything but `licenseKey` and `deviceId`.

AS18. [manual real-app] Activate the same key on a third device and see the "already active on 2 devices" message. Use "Remove licence" and confirm that the Keychain item (account `license`) remains but no longer holds the licence key, while its trial start and device ID are unchanged (R10/R13).

## Design notes (modules, commands/events, data model + migration, UI components)
- **Modules**
  - `src-tauri/src/license/mod.rs`: state, `AccessState`, pure `access_status`.
  - `license/api.rs`: `LicenseApi` trait, with `HttpLicenseApi` (reqwest) and `FakeLicenseApi` (feature `fake-engines`).
  - `license/vault.rs`: thin wrapper over spec 01's `KeyStore` (Keychain impl with account `license`; 01's in-memory fake for tests). No second Keychain trait.
  - A `Clock` trait for injected time.
- **Gate placement.** Call `license::require_access(&state)` at the top of `start_recording` and `transcribe_file` in `commands.rs`. Return `Err(AccessDenied)` as a typed error (extend the current `String` error convention minimally, e.g. a tagged JSON error the wrapper in `src/lib/api.ts` parses). The UI disabled-state is a courtesy; the Rust check is the authority. Electron's UI-only disable plus a main-process check is the same shape.
- **Commands** (`commands.rs`, ts-rs types in `src/lib/types/`, wrappers in `src/lib/api.ts`): `get_access_state`, `activate_license`, `deactivate_license`, `open_external_link` (R11 opens the pricing page through it). Event: `license://changed`.
- **Migration:** none. A `settings` key-value is sufficient: `license.last_verified_at`, `license.expires_at`, `license.last_seen_at`, `license.trial_started_at`, `license.device_id` (the R3/R4 mirrors). If spec 01 introduces SQLCipher before this ships, these keys inherit encryption for free. The Keychain item stays the source of truth for secrets either way.
- **Keychain.** Candidates: the `keyring` crate (mentioned in `plans/TAURI_MVP.md:82`, `:272`, and dropped in the prototype). Or the `security-framework` crate directly. In-house alternative: shell out to `/usr/bin/security`, which puts the secret in a process argument visible to `ps`. That is poor, so reject it. Resolved by sequence: spec 01 lands first, chooses the crate (`security-framework` recommended) in its dependency review, and ships `KeyStore`. This spec reuses it; no new Keychain dependency review.
- **Networking.** Reuse `reqwest` as pinned. No new HTTP crate. The base URL is a compile-time constant; debug builds may read `SCRIBULAR_API_ROOT` (default `http://localhost:3000`, as in Electron `apiConfig.js:7-9`).
- **Device ID and privacy.** Use a **separate** device ID for licensing from any analytics ID (spec 06). Electron uses one ID for both (`usageAnalytics.js:164`, `errorReporter.js:40`), which lets the server join licence, usage and error data. Keeping them separate keeps "validating a licence" distinct from "telemetry", and telemetry can then be opted out without affecting licensing. Flagged for spec 06.
- **Trial conversion (todo.md:8).** The Electron todo asks how to improve trial-to-paid conversion. This spec supplies the hooks only: the `AccessState` machine, the trial banner at ≤ 3 days, a "Buy" button on every denied path, and a gate screen that states notes are safe. It does **not** define the conversion strategy (longer trial, usage-based trial, discount). That needs data from the telemetry spec and a product decision (see Decisions). The Electron gaps are worth fixing in this port: the broken Purchase button, the missing `ACCESS_DENIED` UI, and the warning-restricts-recording bug. Together they likely depressed conversion.
- **Server mismatch notes** (read-only findings for Patrick, no change proposed here): the `/api/validate` route has a test-key backdoor (`route.ts:14`), returns 200 for `valid:false`, and registers a device on *every* successful call from a new device, so trying a key on two devices uses up its quota. The Remove button never frees a slot.
- **UI components:** `Settings.svelte` (License section), `LicenseGate.svelte` (new), `TrialBadge.svelte` (new), and `RecordView.svelte` consumes `accessState`. A small Svelte store, `src/lib/license.ts`, holds the state fed by the event.

## New dependencies (candidates + review needed) or "none"
- **First implementation step:** write `plans/DEPENDENCY_REVIEW_LICENSE.md` and get Patrick's approval before any crate is added.
- Candidate: `keyring` (Keychain access). Why: secure storage of the key is a requirement, and it is a thin wrapper over Security.framework. In-house alternative: `security-framework` bindings directly (a smaller surface, macOS only, which suits the Mac-only decision). Prefer whichever spec 01 picks, so there is one review.
- Candidate: the opener (opening the pricing page). In-house alternative: `std::process::Command::new("open").arg(url)` with a hard-coded allow-listed URL. Fine for a Mac-only app, so no plugin is needed.
- Reused, no new review: `reqwest`, `serde`, `ts-rs`, `rusqlite`. UUID generation is via a few lines on top of existing randomness or the `uuid` crate, so check whether `uuid` is already in `Cargo.lock` first; if it is not, prefer formatting 16 `getrandom` bytes as a v4 UUID in-house.

## Out of scope
- Electron licence or trial migration. Windows. A purchase flow in-app. Server API changes. Telemetry, error reporting and opt-out (spec 06). Auto-update (05). Encryption of the store (01).
- Team or multi-seat licences, refunds, and licence transfer UI.

## Depends on / interacts with (other parity specs by slug)
- **01-encryption-audio-retention:** reuses 01's `KeyStore` and its dependency review. Licence settings keys inherit SQLCipher (01 lands first).
- **06-diagnostics-telemetry:** device-ID separation (above), the privacy copy, and any request logging must mask the key. Trial-conversion measurement depends on it.
- **07-onboarding-samples:** FirstRun should mention the 14-day trial and the "what is sent" line. A first-run network connection for model download is already required, so activation fits there.
- **05-auto-update:** updates must still install for lapsed users (R9 restricts nothing there).
- **08-app-shell-tray-notifications:** the tray's Start Recording item must honour the gate (same Rust check) and show the lapsed message.
- **09-session-management-recovery:** crash recovery of an interrupted recording must work even when lapsed.

## Coordination notes (gap-check 2026-11-04)
- **No runtime bypass, ever.** Electron's packaged build honours `--e2e-test` (licence `hasAccess:true`, `licenseService.js:196-199`; `E2E_VALID_LICENSE_KEY`, `:29-31`) and `--dev` + `SCRIBULAR_SKIP_LICENSE=1` (`index.js:54-60`, `licenseService.js:25-27,192-194`), so `open -a Scribular --env SCRIBULAR_SKIP_LICENSE=1 --args --dev` skips the licence in production. R14's rule stands: any override is `cfg(debug_assertions)` or a Cargo feature, never a CLI flag or env var read in a release build. Add a test that a release-profile build ignores `SCRIBULAR_SKIP_LICENSE`.
- **Keychain:** one item, service = bundle identifier, account `license`, via 01's `KeyStore`. The identifier must not change after release (decision in spec 05).
- **No migration** in this spec (settings KV only).
- **External links:** 04 introduces `open_external_link` (see R11). 05, 06, 07 and 08 reuse it.
- **Gate UI:** in-app only (`LicenseGate`), no native dialog, consistent with 08 R16. The licence section is a headed section in the current Settings view; 08 later moves it into its category layout. Log masking: before 06 lands, log only the masked key through the existing logging; 06 converts it.

## Decisions for Patrick (with recommended default)
1. **Offline grace length.** Electron has effectively unlimited offline use. Default: 30 days grace plus a 7-day soft period (R7).
2. **Trial reset resistance.** The trial lives in the app data and a Keychain mirror. It is not server-enforced, so a determined user can restart it. Default: accept this, with no server change. A server-side trial would send a device ID at first launch before any licence purchase, which weakens the privacy promise.
3. **Remove the `testing-success-shoemaker-392` backdoor from `/api/validate`.** It returns a valid, 2099-expiry licence in production. Default: remove it, and move tests to a mock. This is a server change outside this repo; it needs a decision and a separate task.
4. **Separate device IDs** for licence and analytics (above). Default: separate.
5. **Trial length and conversion tactics** (todo.md:8). Default: keep 14 days and ship this spec's hooks. Decide tactics once spec 06 produces data.
6. **Free a device slot.** Remove Licence cannot release the server-side slot (2 devices max). Default: leave it and have support free slots manually. A proper fix is a server `/api/deactivate`, which is out of scope.

## Size (S/M/L, rough days) and suggested PR split if > ~1 PR
**M, about 5 to 7 days.**
1. PR1 (S, 1 d): `DEPENDENCY_REVIEW_LICENSE.md`. Wait for approval.
2. PR2 (M, 2-3 d): the `license` module (trial, state machine, vault wrapper over 01's `KeyStore`, API trait and fake), the gate in `commands.rs`, and tests AS1-AS12.
3. PR3 (S-M, 2 d): commands and events, the ts-rs types and `api.ts` wrappers, the Settings licence section, `LicenseGate`, the badge, and vitest AS13. Then the manual checks AS14-AS18.

## Execution notes (Patrick via Darby, 2026-11-05)
- **Reviewer is Cubic, not Knightwatch.** The cwf task is set to `reviewer=cubic`. Use the configured `cwf review` / `cwf kw` commands only; never run Knightwatch quota or request commands. Handle every live Cubic finding: fix with a regression check, or answer with precise evidence and resolve the thread using the agent-reply marker.
- **Spec 01 is merged** (main `9e17ac0`). Reuse its `KeyStore` / `KeychainKeyStore` (account-parameterised) and its SQLCipher store. Do not add a second Keychain abstraction or crate.
- **Dependencies.** Step 1 is still `plans/DEPENDENCY_REVIEW_LICENSE.md`. If the work needs no new crate (uuid formatted in-house from `getrandom` or an existing `uuid`, `open` via `std::process::Command`, `reqwest` reused), record "none" and carry on. If any new crate is needed, write the review, transition to `blocked`, and wait for Patrick.
- **Parallel work.** Spec 02 (diarization, PR #8) is being built by another agent in `/Volumes/My Shared Files/Scribular/scribular-tauri`. Do not touch that checkout. Work only in this clone (`scribular-tauri-p04`). Expect overlap in `commands.rs`, `src/lib/api.ts` and `Settings.svelte`: keep changes minimal there, and rebase on `origin/main` before opening the PR.
- **Decisions for Patrick:** proceed on the recommended defaults in the spec. The server-side `testing-success-shoemaker-392` backdoor is a separate server task and is out of scope.
- **Manual checks AS14-AS18.** Do what can honestly be done on this VM (release build, Keychain item, clock/offline checks). Where a check needs a real licence key, a third device, a throwaway macOS user, or Little Snitch, say so plainly in the verification record. Do not fabricate evidence. Follow cwf's screenshot budget.
- No release, signing or merge work. Product approval and merge stay with Patrick.
