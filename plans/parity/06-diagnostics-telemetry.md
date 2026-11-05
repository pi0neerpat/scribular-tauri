# Diagnostics, error reporting and telemetry — Tauri parity spec
Status: draft for cwf; Date: 2026-11-03


> **Dependency policy (Patrick, 2026-11-05; supersedes every dependency-review, Patrick-approval, publish-date cutoff, two-week rule, exact-pin, OSV/RustSec, owner-check or `build.rs`-review requirement in this spec and in `plans/DEPENDENCY_REVIEW_*.md`).** Before adding a crate or npm package, check only that (1) it is commonly used (substantial downloads or dependents) and (2) it has had a release or commit within the last 12 months. Record the result in one or two lines in the PR description. No `DEPENDENCY_REVIEW_*.md` doc and no approval step. Prefer dependencies already in the tree. This covers packages only: a pinned model file still needs its URL, size and sha256 taken from the upstream file, never invented.

## Intent (why users need it; who)
- Support needs a way to see what went wrong on a user's Mac: a log file, a "Copy diagnostics" blob, and (optionally) error reports that reach the maintainer.
- Patrick needs coarse usage numbers (launches, sessions, average length per week) to know whether the product is used.
- Users are therapists. **Nothing clinical may leave the device, and nothing clinical may land in a log file either**, because users paste logs and diagnostics into support emails. Opting out of telemetry must be honoured before the first event, not after.

## Electron behaviour today (with file:line refs)
Repos: Electron `notes-assistant-electron/src`, backend `scribular-saas-project`.

### Endpoints and services contacted for diagnostics/telemetry
There are exactly two, both on the Scribular backend. The root comes from `utils/apiConfig.js:6-10`: `http://localhost:3000` when `global.debug`, else `https://www.scribular.com`. No third party (no Sentry, no analytics SDK) appears in `src/`; a grep for `fetch(` / `https?://` finds only these two, `/api/validate` (licensing, spec 04), the local Ollama host and web links.

1. `POST {root}/api/errors` — `utils/errorReporter.js:42-45` (and fallback `:51-54`). Handler: `scribular-saas-project/app/api/errors/route.ts`.
2. `POST {root}/api/usage` — `utils/usageAnalytics.js:190-194`. Handler: `scribular-saas-project/app/api/usage/route.ts`.

### Payload: `/api/errors`
Built in `errorReporter.js:67-80`:
```
{ context, message, metadata: { version, platform, arch, ...extra }, timestamp, deviceId }
```
- `deviceId`: random UUID created once (`store/store.js:16-21`), the same id used for licensing. It links all reports and usage rows to one install.
- `context`: one of `ollama-serve, ollama-run, chat, summarize, recording, notification, dialog, unhandled` (`errorReporter.js:63`; the server rejects anything else, `route.ts:10-19,67-72`).
- `message`: whatever error text the caller had. The server truncates to 500 chars (`route.ts:7,103`) but the client sends it whole.
- `metadata.extra`: `{ stack }` for unhandled errors (`index.js:8`, `registerIpcHandlers.js:333` adds `process: "renderer"`), `{ title }` for dialogs (`audio.js:214…529`), `{ test: true }` for the test button (`registerIpcHandlers.js:322`). Server caps metadata at 5,000 chars and truncates `stack` (`route.ts:8,81-96`).
- Callers: `api.js:185,235,269` (Ollama/chat errors), `summarize.js:153,171,507`, `audio.js:55,214,238,266,276,320,405,408,529`, `registerIpcHandlers.js:310` (any notification text matching `/error|failed|invalid|please try again/i`), plus unhandled main-process and renderer errors (below).

### Payload: `/api/usage`
`usageAnalytics.js:156-175`:
```
{ deviceId, sessionsThisWeek (<=200), avgSessionDurationMinutes (<=480, 2dp),
  appLaunchesThisWeek (<=500), appVersion, platform: "darwin-arm64", weekStart: "YYYY-MM-DD" (UTC Monday) }
```
Counters persist in `userData/usageAnalytics.json`, reset each week (`:39-69`). `recordSession` takes only the duration (`:129-131`: "durationSeconds only — no content"). Server validates UUID and upserts by (deviceId, weekStart) (`usage/route.ts:62-82`). Triggers: app start and every 3 h (`index.js:287-301`), first ever saved session (`registerIpcHandlers.js:124,161-167`), and window close (`index.js:573-580`). `recordLaunch` at `index.js:416,605`.

### Is any clinical content sent?
- Usage: **no**. Numbers, version, platform, install UUID only.
- Errors: **no content field is added deliberately, but nothing prevents it.** `message` is free-form text from other layers and is never sanitised:
  - Summarisation errors are derived from the LLM runtime's error object (`summarize.js:467-501`, `reportError` at `:507`). I could not prove such errors never quote prompt text (the prompt contains the transcript). Treat as a risk.
  - Renderer `window.onerror`/`onunhandledrejection` forward `error.message` and the stack (`renderer/client.js:9-22`). A `JSON.parse` or string-handling exception message can include a snippet of the data being parsed.
  - Stacks contain absolute file paths, which include the macOS username.
  - Notification text is reported if it looks like an error (`registerIpcHandlers.js:309-310`); today those are fixed strings.
- **Logs on disk**: `utils/logger.js:13-17` writes `~/.scribular/app.log` (500 KB, `maxFiles: 1`; what winston does on rollover with `maxFiles: 1` is not verified). The grep of `logInfo/logErr` calls shows lengths and word counts, not text (`summarize.js:138,143`, `audioWorker.js:671`, `ollama.js:344`). But `logErr("Error processing transcript:", error)` (`summarize.js:109,168`) logs full error objects, with the same quoting risk as above. Prompt/style names are logged (`summarize.js:217,225`; user-authored, usually not clinical).

### The opt-out is only partly respected (Electron bugs, do NOT copy)
- `telemetryEnabled` defaults true (`store/defaults.js:82`); toggle in settings (`settings.js:769,894,910`).
- It gates only the start-up/3-hourly send (`index.js:292`).
- **Not gated**: error reports (`errorReporter.js` never reads the setting), the quit-time usage send (`index.js:575-579`), the first-session send (`registerIpcHandlers.js:161-167`). Counters are also recorded when disabled.
- Nothing is asked before the first send; the first send happens at app start with the default true.

### Capturing unhandled errors
- Main process: `electron-unhandled` initialised at top of `index.js:3-13`; its logger calls `reportError("unhandled", message, {stack})`; `showDialog` is on except under `--e2e-test`. (The default dialog text comes from the library, not read here.)
- Renderer: `window.onerror` / `onunhandledrejection` → IPC `APP_REPORT_UNHANDLED_ERROR` (`client.js:1-23`, `registerIpcHandlers.js:332-335`). No dialog.
- `--e2e-test` disables reporting and analytics (`errorReporter.js:15-17`, `usageAnalytics.js:110,132`).

### User-visible actions
- **Copy diagnostics** (`settings.js:1042-1054`): `getDiagnostics` returns a 6-line text: `Scribular <ver>`, `OS: darwin (arm64)`, Electron, Chrome, Node, V8 versions (`registerIpcHandlers.js:100-112`), copied to clipboard, tooltip "saved"; failure → notification "Failed to copy diagnostics."
- **View logs** (`settings.js:1056-1068`, `registerIpcHandlers.js:289-303`): `shell.openPath(~/.scribular/app.log)`. Note `openPath` resolves with an error string rather than throwing, so the error dialog is probably unreachable; and a missing file gives no feedback (unclear/buggy).
- **Developer section** (debug builds only, `global.debug`): test error report and test unhandled exception buttons (`settings.js:1091-1114`, `registerIpcHandlers.js:320-331`).

## Tauri state today (what exists, what is missing, with file refs)
- No logger. Diagnostics are `eprintln!` to stderr only: `jobs/queue.rs:112,217,234,302,303,348,355`, `jobs/summarize.rs:84,263`, `jobs/session.rs:262,279,576,615`, `audio/mic.rs:146`, `audio/system.rs:370`, `audio/chunk_writer.rs:84`. A packaged app has no visible stderr, so users have nothing to send. These print error values and ids, not transcript text (grep of `eprintln!` for transcript/text/summary found only `bin/bench.rs:225`, a dev tool).
- No panic hook. Only `catch_unwind` in the job queue (`jobs/queue.rs:7,329` → `summarize::fail_after_panic`, `summarize.rs:117`). `lib.rs:114` ends with `.expect("error while running tauri application")`.
- No webview error handlers (no `onerror` in `src/`).
- No telemetry, install id, or `telemetry_enabled` setting. Settings is a KV table (`store/settings.rs:27-47`, `get_setting`/`set_setting`); migrations are `store/migrations.rs:11` (v1-v3).
- `commands.rs` has `app_version` (`:85`) and `open_privacy_settings` (`:226`, shells out to macOS) but no diagnostics/logs command. IPC test `lib.rs:154-171` forces every command to have an `api.ts` wrapper.
- Settings UI: `src/lib/components/Settings.svelte` (only recording settings).
- Only network use is model download via `reqwest` (`models/download.rs`, `reqwest 0.13.5` native-tls, `Cargo.toml:29`). CSP `default-src 'self'` (`tauri.conf.json:21`) blocks webview-originated requests, so all telemetry must go through Rust.
- llama/whisper (ggml) write their own logs to stderr; `DEPENDENCY_REVIEW_ENGINES.md:205-207` already notes the log callback can be redirected.

## Goals / Non-goals / Anti-goals
Goals
- Content-free local log file with rotation, panic and webview-error capture, View logs, Copy diagnostics.
- Optional remote error reports and weekly usage counters, wire-compatible with the existing backend routes, with a real opt-out gate.

Non-goals
- Session-replay, per-event analytics, crash dumps (minidumps), remote log upload, in-app log viewer.
- Backend changes (spec works against the existing `/api/errors` and `/api/usage`).
- Migrating the Electron `deviceId` or `usageAnalytics.json` (fresh start).

Anti-goals
- Never copy Electron's free-form `message`/`stack` reporting. Never send, log, or put in diagnostics any transcript, summary, prompt text, session title, speaker name, file path containing a username, or error text that may quote them.
- Never send anything before the opt-out state is known and enabled.
- Never let logging or telemetry failure crash, block or slow the app.

## Requirements (numbered R1..)
**Dependencies**
- R0. Default is zero new crates; any crate added must pass the dependency policy at the top of this spec.

**Logging**
- R1. A `diag::log` module writes lines `ISO-8601 LEVEL [module] message` to `<app_data_dir>/logs/scribular.log` (Spec author's call: data dir as the brief asks; one place to purge; not `~/Library/Logs`). Directory mode 0700, file 0600. Also mirrors to stderr in debug builds.
- R2. Rotation: when the active file passes 1 MiB, rename `scribular.log`→`.1`→`.2`→`.3`, dropping the oldest, so at most ~4 MiB (Spec author's call: Electron's 500 KB × 1 file loses the lines just before a crash). Rotate at open as well as at write time.
- R3. Startup banner logs app version, macOS version, arch (as Electron `index.js:18`). Initialised first in `run()` before `Builder`.
- R4. Content-free by construction: the log API accepts `&str` messages but call sites may log only ids, counts, durations, model ids, error *kinds*. Provide `diag::kind(&dyn Error)` helper that returns a short enum-like label; error `Display` strings from llama/whisper/SQLite/reqwest-with-URL are not logged raw. A test (AS7) enforces it with a canary.
- R5. Convert the existing `eprintln!` sites (list above) to `diag::log`. Redirect ggml/llama/whisper log callbacks to the logger at level WARN+ only (drops verbose token/prompt output; see ENGINES review).
- R6. A logging failure (disk full, permission) is swallowed after one stderr note.

**Panic and webview capture**
- R7. `std::panic::set_hook` installed in `run()` first. It logs `PANIC thread=<name> at <file>:<line>:<col>` plus the payload *type* only. It does **not** log the payload text (Rust string-slicing and `unwrap` panics can embed the string being processed). It chains to the previous hook in debug builds. It never panics itself.
- R8. Panics inside `catch_unwind` (job queue) also pass through the hook, then keep the existing `fail_after_panic` behaviour.
- R9. Webview: `src/main.ts` registers `window.onerror` and `onunhandledrejection`, calling command `report_frontend_error { kind, name, frames }` where `frames` is `file:line:col` entries with the origin stripped. The error `message` is **not** sent to Rust (Spec author's call: it can quote data). Rust logs it and (if remote enabled) queues a report with code `webview:<name>`. Rate-limit: max 20 per minute, then drop with one "suppressed N" line.
- R10. A webview-side unhandled error shows no dialog (parity with Electron renderer). A Rust panic on the main/job thread logs and, for a main-thread panic, lets the process abort as Tauri does (Electron shows a dialog; Tauri has no equivalent and a dialog on the panic path is unsafe).

**Actions**
- R11. Command `get_diagnostics() -> String` returns, one per line: `Scribular <version> (build <git sha if available>)`, `macOS <version> (arm64)`, `Tauri <tauri::VERSION>`, `DB schema v<n>` (`Store::user_version`, `store/mod.rs:95`), installed model ids with status (from `ModelManager`), `Telemetry: on|off|undecided`, `Log dir: ~/Library/Application Support/<id>/logs` (home replaced by `~`). No counts of sessions, no titles, no paths with the username.
- R12. "Copy diagnostics" button copies that text to the clipboard and shows the same brief confirmation as Electron; failure shows an inline error.
- R13. Command `view_logs()` creates the log file if missing and runs `open <file>` (same pattern as `commands.rs:226`); returns `Err(String)` if `open` fails and the UI shows an inline error (fixing Electron's swallowed error).

**Remote reporting and opt-out**
- R14. Setting key `telemetry_enabled` (settings KV): absent = undecided, `"1"`/`"0"` otherwise. **Undecided and `"0"` both send nothing.** The key is written during FirstRun (see Decisions) and editable in Settings → Privacy.
- R15. One chokepoint `telemetry::send_allowed(&Store)` reads the setting from the store on every call (no cached copy) and gates *all* of: usage counters recording, usage send, error report send. Quit-time and first-session sends use the same gate (fixes Electron `index.js:575`, `registerIpcHandlers.js:161`). Turning the toggle off discards unsent queued reports and zeroes counters.
- R16. Install id: random UUID v4 in setting `install_id`, created lazily on first *allowed* send, never before. Not shared with licensing unless spec 04 chooses to. Deleted when telemetry is turned off (Spec author's call: unlinks past data; a new id is made if re-enabled).
- R17. Usage payload identical in shape to Electron (`deviceId, sessionsThisWeek, avgSessionDurationMinutes, appLaunchesThisWeek, appVersion, platform:"darwin-arm64", weekStart`) with the same clamps. Counters live in settings keys (`usage_week_start`, `usage_sessions`, `usage_total_seconds`, `usage_launches`). Triggers: launch (after gate), every 3 h while running, first saved session, app quit (best effort, 2 s cap). A "session" is counted once at transition to a saved transcript, not on summary completion (Electron `registerIpcHandlers.js:149-159`).
- R18. Error report payload: `{ context, message, metadata:{version,platform,arch,code}, timestamp, deviceId }`. `context` is from the server's closed list; `message` is a **fixed string from a closed Rust enum** (`"summarize failed"`, `"recording failed"`, `"model download failed"`, `"panic at src/jobs/queue.rs:329"`, `"webview TypeError"`). No stack, no error text, no titles. This works with the unchanged server (`route.ts:58-79`). Mapping: summarise/LLM→`summarize`; capture/transcribe→`recording`; model download→`dialog`; panic/webview→`unhandled`.
- R19. All requests are made by Rust (`reqwest`, already a dependency), 10 s timeout, fire-and-forget on a background thread, max 30/min and de-duplicated by code per 10 min. Panic reports are written to the log immediately and sent at the **next launch** (if allowed), not from the panic handler.
- R20. API root: release builds `https://www.scribular.com`. Debug builds send nothing unless env `SCRIBULAR_API_ROOT` is set (Electron defaults debug to localhost:3000, `apiConfig.js:7-9`). Tests inject the root.
- R21. Developer section, debug builds only: "Send test error" (`report_test_error`) and "Trigger test panic" (`trigger_test_panic`); both compiled out with `cfg(debug_assertions)`.

**Wiring**
- R22. New commands registered in `lib.rs` `generate_handler!`, with wrappers in `src/lib/api.ts` (enforced by `lib.rs:154-171`) and ts-rs types for any struct.

## Acceptance scenarios (numbered AS1..)
- AS1 (automated, cargo test): Given a temp data dir, when 3,000 lines of 400 bytes are logged, then `scribular.log` ≤ 1 MiB, files `.1`–`.3` exist, `.4` does not, and the newest line is in `scribular.log`.
- AS2 (automated, cargo test): Given the panic hook installed, when a thread panics with message `"CANARY-CLIENT-JANE"` (including via string-slice panic on a canary string), then the log contains the file:line and no `CANARY`.
- AS3 (automated, cargo test): Given `telemetry_enabled` absent, when the app init sequence runs (recording launch, saving a session, reporting an error, quit hook) against a local `TcpListener` as API root, then the listener receives zero connections and no `install_id` setting exists.
- AS4 (automated, cargo test): Given `telemetry_enabled="1"`, when a session is saved and quit is signalled, then exactly the usage JSON of R17 arrives with a valid UUID, clamps applied, and keys equal to the Electron set.
- AS5 (automated, cargo test): Given `"1"` and a pending error report, when the setting flips to `"0"` before the sender runs, then nothing is sent, counters are zero, and `install_id` is deleted.
- AS6 (automated, cargo test): Given a fake-engines summarisation that fails with an error whose text contains `CANARY-CLIENT-JANE`, when the report is built, then the request body, log file and `get_diagnostics()` output contain no `CANARY`, and `message` is one of the closed strings.
- AS7 (automated, vitest): Given `window.onerror` is triggered with an Error whose message is a canary, then the mocked `invoke('report_frontend_error')` argument contains `name` and `frames` and no canary.
- AS8 (automated, cargo test): `get_diagnostics()` has the 6 expected lines and no `/Users/<name>` path.
- AS9 (automated, cargo test): Webview error flood (100 in 1 s) produces ≤20 log entries plus one "suppressed" line.
- AS10 (manual real-app): Fresh install, quit before choosing at FirstRun, and watch traffic (Little Snitch/`nettop`): no connection to scribular.com. Then choose "off", record and summarise a session: still none.
- AS11 (manual real-app): Settings → Privacy → Copy diagnostics, paste into a text editor: text matches R11. View logs opens `scribular.log` in the default app and it shows the startup banner.
- AS12 (manual real-app, debug build): "Trigger test panic" → relaunch shows PANIC line in the log; "Send test error" with `SCRIBULAR_API_ROOT` pointing to a local netcat shows the R18 body.
- AS13 (manual real-app): Record a session containing a spoken canary name, summarise, open the log file, search for the name: not found.

## Design notes (modules, commands/events, data model + migration, UI components)
- New `src-tauri/src/diag/` : `log.rs` (file writer + rotation + level), `panic.rs` (hook), `report.rs` (closed enum `ReportCode`, payload builder), `telemetry.rs` (gate, counters, sender thread, quit hook), `info.rs` (diagnostics text). Pure functions take `&Store`/a clock/an API root so tests need no Tauri runtime.
- `lib.rs`: call `diag::init(data_dir)` before `Builder`; telemetry thread started in `setup` after `Store::open`; quit send in `.run(|app, event| … RunEvent::ExitRequested …)` (replace the trailing `.run(generate_context!())` accordingly).
- `commands.rs`: `get_diagnostics`, `view_logs`, `report_frontend_error`, `get_telemetry_setting`, `set_telemetry_setting`; debug-only `report_test_error`, `trigger_test_panic`.
- Data model: settings KV keys only (`telemetry_enabled`, `install_id`, `usage_*`, `pending_report`). **No schema migration** (store keeps `user_version` 3). If spec 01 moves to SQLCipher the keys move with the DB.
- `src/main.ts`: handlers (R9). `src/lib/api.ts`: wrappers. `Settings.svelte`: new "Privacy & diagnostics" section with toggle, Copy diagnostics, View logs; debug-only Developer rows.
- Tauri capabilities: no new plugin permissions (app commands are allowed by default, `capabilities/default.json`). Clipboard: `navigator.clipboard.writeText` from a click handler, as Electron.
- Closed-set error mapping lives in one `ReportCode` enum so adding a code is a compile-time change and reviewers see every string that can leave the device.

## New dependencies (candidates + review needed) or "none"
Default: **none** (reqwest, serde_json, std are enough). No review doc is needed.
- `tauri-plugin-log` (+ `log`, `fern`): rotation and webview→Rust log forwarding for free. Rejected by default: pulls a plugin permission surface and `log` macros that make it easy to log a `{e}` string raw; in-house is ~150 lines.
- `tracing` + `tracing-appender`: heavier than needed; daily rotation only, no size rotation.
- `sentry`/`tauri-plugin-sentry`: rejected; third-party service for therapy data, contradicts the closed-payload requirement.
- `uuid` (for install id): in-house alternative is formatting 16 random bytes; `getrandom` is already transitive. Only add if the review prefers it.

## Out of scope
- Backend dashboards/admin for `appError` and `usageStats`, any server change.
- Crash minidumps, remote log upload, in-app log viewer.
- Licence/deviceId sharing (spec 04), update check telemetry (spec 05).
- Windows paths.

## Depends on / interacts with (other parity specs by slug)
- 07-onboarding-samples: the telemetry choice is a FirstRun/onboarding step; this spec defines the setting and gate, 07 places the UI (fallback: Settings toggle only, telemetry stays "undecided" = off).
- 04-licensing-trial: Electron reuses `deviceId` for `/api/validate`; if 04 introduces an id, it must stay separate or be sent only by 04's own consented call.
- 01-encryption-audio-retention: logs are plaintext outside the encrypted DB, which is why R4 forbids content; settings keys move into the encrypted DB.
- 05-auto-update: any update-check request must also obey `telemetry_enabled` only if it transmits the install id (preferably it doesn't).
- 09-session-management-recovery: the "interrupted" Failed status could produce a report code, content-free.

## Coordination notes (gap-check 2026-11-04)
- **Order:** 01, 04 and 05 land first. Settings keys (`telemetry_enabled`, `install_id`, `pending_report`) live in 01's encrypted DB, so start the telemetry thread only after unlock. While the store is Locked, nothing is sent (undecided = off). Copy diagnostics may include 01's `cipher_version` and lock reason, never the key.
- **Converts earlier logging:** 04 (masked licence key) and 05 (update outcomes) log with `eprintln!` until this spec lands. Move them onto `diag` here and keep the masking.
- **FirstRun consent:** 07 lands after this spec. Ship the Settings → Privacy toggle here (telemetry stays undecided = off); 07 adds the FirstRun choice. AS10's FirstRun part is re-run in 07.
- **No runtime test switch:** Electron turns reporting off under `--e2e-test` at runtime (`errorReporter.js:15-17`). Here, tests use a fake transport behind `cfg(test)`/`fake-engines`; there is no CLI flag or env var.
- **Openers:** `view_logs` opens a local file with `open` (R13). It is not a URL, so it does not go through 04's `open_external_link` allow-list. Keep it a separate command. The Privacy section is a headed section in the current Settings view; 08 later moves it into its category layout.

## Decisions for Patrick (with recommended default)
1. **Default for telemetry.** Electron defaults ON with no prompt. Recommended: show an explicit choice at FirstRun with the toggle pre-set to ON, send nothing until the user continues (undecided = off). If you want stricter: pre-set OFF. Work proceeds on the first.
2. **Remote error reports at all?** Recommended: yes, closed-set codes only (R18), same toggle as usage. Alternative: logs/diagnostics only, no remote errors.
3. **Install id shared with licensing?** Recommended: no, separate id (R16).
4. **Log location.** Recommended: `<app_data_dir>/logs` as scoped; alternative `~/Library/Logs/com.app.scribular.tauri` (Console.app-friendly).

## Size (S/M/L, rough days) and suggested PR split if > ~1 PR
M, about 5 days.
- PR1 (2.5 d): R0, R1-R13, R22, AS1,2,7,8,9,11,13. Local only, no network.
- PR2 (2.5 d): R14-R21, AS3-6,10,12, Settings Privacy section; needs decision 1 and, for the FirstRun step, a hand-off to spec 07.
