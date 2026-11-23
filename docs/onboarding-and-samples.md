# Onboarding and sample session (parity spec 07, PR #17; onboarding PR1, PR #43)

Spec: `plans/2026-11-16-onboarding-spec.md`. PR1 covers AS1-AS3 only.

## What it does
- First launch shows one **Welcome to Scribular** sheet (`WelcomeSheet.svelte`). It is the only first-run prompt: three points (record a conversation, notes in your style, stays on this Mac), an unchecked **Share anonymous usage and crash data** checkbox, and **Continue**. The sheet cannot be dismissed with Escape.
- The old four-step tour (`OnboardingTour.svelte`), the telemetry prompt (`FirstRun.svelte`), the **Show tour** button, and the "Getting Scribular ready" model checklist (`ModelStatus.svelte`, `SpeechModelStatus.svelte`) are removed.
- Usage-data consent stays off until the user checks the box. Showing the sheet saves consent off first, then marks welcome seen. The sheet is shown again on relaunch if either write failed.
- Help > welcome (menu event `shell-welcome`) reopens the sheet with the saved choice. Changing the checkbox saves immediately; a failed save shows an error and **Retry** and keeps the previous choice.
- Record shows the speech model state next to **Start recording**: a progress ring around the button and a status line (Downloading the speech model with a percent, Verifying, or the failure message). **Start recording** is disabled until the speech model is ready. A failed download, or a failed status/download call, shows **Retry speech model download**.
- The summary model downloads in the background with no global wait message or checklist. Only a session already in `awaitingModel` shows "Waiting for the summary model download…".
- One example session (a labelled therapy-style transcript and summary, 1,245 s, dated one day back) is seeded once. Regenerating the summary of a sample is rejected.

## How it works
- `WelcomeSheet.svelte` (props `open`, `suspended`; exports `show()`): on mount `getOnboardingState`. If not completed it opens the modal `<dialog>` and, once not suspended, runs `saveFirstDisplay`: `setTelemetrySetting(false)`, then `completeOnboarding()`. The controls stay disabled until that finishes. If a read or write fails it shows an alert and a retry (the retry reruns the same action); welcome stays unseen. `show()` reads the saved opt-in with `getTelemetrySetting` and makes no writes. A checkbox change calls `setTelemetrySetting(next)` and only updates the checked state after it succeeds.
- `App.svelte` mounts `WelcomeSheet` with `suspended={!!quitIntent}` so quit confirmation takes focus. While `welcomeOpen`, window shortcuts are ignored and a tray start brings the main window forward (`setMainWindowVisible(true)`).
- Model progress (`App.svelte`): once the license gate is open it registers `onModelsProgress`, loads `modelsStatus()`, merges with `mergeSnapshot` (`src/lib/models.ts`, live events win over the snapshot), then calls `downloadModels()`. `speech` is the `whisper-base` entry. `preparingModels` ends on that model's `ready` or `failed`; an IPC failure sets `modelError`. `retryModels` reruns the same routine and is passed to `RecordView` as `onRetryModels`.
- `RecordView.svelte`: `speechReady` and `speechPercent` (downloaded / total bytes, capped at 100) drive the ring (`.record-ring`, CSS variable `--speech-progress`) and the status line. Starting is gated by `speechReady` plus the existing gates (source, microphone permission, license); a tray start while not ready fails with "The speech model isn't downloaded yet.".
- Backend `ModelManager` (`src-tauri/src/models/mod.rs`) keeps the latest `ModelProgress` per model in a `progress` map, updated with `readiness` in `report`. `status()` returns that entry when its state matches the readiness, so a reload or snapshot after a reconnect sees real byte counts. A partial `.part` file resumes with a range request rather than restarting.
- Commands: `get_onboarding_state`, `complete_onboarding`, `set_telemetry_setting`, `restore_sample_sessions` (`src-tauri/src/commands.rs`; wrappers in `src/lib/api.ts`). `store/samples.rs` seeds the sample in one transaction. `SampleBadge.svelte` and `SampleBanner.svelte` exist but are not yet shown in a list or detail view.

## Config and data
- Settings keys (`settings` table): `onboarding_completed` (`ONBOARDING_COMPLETED`, `"1"` once welcome is seen; absent means unseen), `telemetry_enabled` (`TELEMETRY_ENABLED`, `diag/telemetry.rs`; `"0"` off, `"1"` on, and `send_allowed` is true only for `"1"`, so an unset value never sends), and `SAMPLES_SEEDED` (`"1"` once seeded, so a deleted sample is not re-added).
- Defaults: consent off; welcome unseen. Consent becomes on only when the user checks the box and the save succeeds. Turning it off clears the install id and pending counters (`set_enabled`).
- No new migration in PR1. Migration v6 added `sessions.is_sample INTEGER NOT NULL DEFAULT 0`; existing sessions stay non-samples.
- Restore: `restore_samples` deletes all `is_sample = 1` rows and inserts a fresh one.

## How to test
- `npm run check && npm test && npm run build`. `src/onboarding.svelte.test.ts` covers AS1 (one sheet, off saved before seen, reload during download, tray start and quit focus), AS2 (Help reopen, opt-in only after an explicit successful save, read and save failures) and AS3 (newest progress beats a stale snapshot, quiet summary download, retry after failure, retry after a status IPC failure). `src/lib/components/record-and-sample.test.ts` covers Record and the sample.
- Rust: `cargo test --manifest-path src-tauri/Cargo.toml --test models_download` (resume with a range request keeps `status()` in step with progress; a checksum failure reports `Failed`) and `cargo test --manifest-path src-tauri/Cargo.toml store::settings` (`failed_welcome_writes_leave_consent_off_and_welcome_unseen_on_reopen`, once per failing key). Use `--features fake-engines` for the lib tests, and set `TAURI_CONFIG='{"bundle":{"macOS":{"frameworks":[]}}}'` if the frameworks bundle is unavailable.
- Native: see CLAUDE.md "E2E / uitest"; pass `--app` the `.app` bundle path, not the `release` directory. Build a debug bundle and set `SCRIBULAR_SKIP_LICENSE=1` in uitest's environment (debug builds only). Scenarios dismiss the sheet with `click "Continue"` then `expect-not "Welcome to Scribular"` (`tools/uitest/scenarios/note-style-coach.uitest`).

## Known limits
- AS4-AS10 are not shipped: PR2 (first On a call explainer, OS permission status, AS4-AS5, AS8) and PR3 (first-note tip, Note Styles empty card, AS6-AS7, AS9-AS10) are still to do.
- AS4 and AS9 are manual real-app checks on a fresh macOS permission state or clean profile; they have not been run.
- Sample badge/banner and Copy, and deleting the sample, need the session list/detail UI from spec 09; only backend and component tests cover them.
- Only one sample, not the three Electron defined.
- Start recording, model-download permission rows and launch-to-ready baseline were not verified on the final head (task handoff AS11).

# On a call explainer and app-audio permission (onboarding PR2, #50)

## What it does
- The first time **On a call** is chosen, a modal sheet explains that app audio needs Scribular enabled in macOS Screen & System Audio Recording. Later selections go straight to the app picker.
- The sheet shows `Granted` or "Permission unconfirmed". It offers Open System Settings, Choose an app / Try recording (Continue with selected app once granted), and Not Now.
- **Not Now** (or Escape) switches the source back to In the room; no app audio is captured. The sheet closes after a successful start.
- The "Retry app audio" button (permission required or app unavailable while recording) reopens the sheet; it does not auto-open after relaunch.

## How it works
- `RecordView.svelte` reads the flag with `getOnboardingState`, marks it on first display (`markAppAudioExplainerShown`), and polls `systemAudioPermissionStatus` every 2 s and on focus while the sheet is open and visible.
- System Settings opens only when the user clicks Open System Settings (`openPrivacySettings('systemAudio')`), never automatically.
- `system_audio_permission_status` (`commands.rs`) uses `CGPreflightScreenCaptureAccess`: it returns `granted` or `unknown`, never `denied`, because macOS cannot tell denied from unrequested. The UI claims a grant only from preflight or a live capture (`appAudioStatus === 'capturing'`).
- `capture.rs` publishes `AppAudioStatus` (`pending`, `capturing`, `permissionRequired`, `unavailable`) on `RecordingState.appAudioStatus`.

## Config and data
- Setting `app_audio_explainer_shown` (`"1"`, `store/settings.rs`) is persisted once, idempotently, and exposed as `OnboardingState.appAudioExplainerShown`. No migration.
- New commands: `mark_app_audio_explainer_shown`, `system_audio_permission_status`.

## How to test
- `npm test` (`record-view-speaker-count.svelte.test.ts`, `api.test.ts`) and `cargo test --lib --features fake-engines` plus `cargo test --test session_job` (flag idempotence and failed write keep data, AS8; mic or mic+app modes, AS5).
- AS4 is manual only, on a fresh macOS permission state: choose Not Now, reopen via Retry app audio, grant in System Settings, and confirm the sheet flips to Granted without a click. Record states and screenshots.

## Known limits
- Denied cannot be shown distinctly; the sheet stays "unconfirmed" until preflight reports granted or a real capture succeeds.
- AS4 has no automated coverage.

# First-note tip, Note Styles card and telemetry-off privacy (onboarding PR3, #57)

## What it does
- After the first non-sample session completes a note, `src/lib/components/SummaryPanel.svelte` shows "Want notes that look like yours?" beside the StylePicker, with a coach entry (New Style from Example Note...) and Dismiss tip. Sample notes never trigger it.
- The coach entry follows the shared coach availability gate; while recording it is disabled with its reason. Using the entry or dismissing the tip persists dismissal, and the tip never returns.
- Note Styles shows the "Make notes that look like yours" card above built-ins while no custom style exists, with two entries into the note-style-coach route: New Style from Example Note... (Step 1) and Use a Sample Note (sample chooser open). The card is not dismissible and disappears once any custom style exists (`src/lib/components/note-styles.svelte.test.ts`).
- If saving the dismissal fails, the tip stays and shows "Couldn't save the dismissal. Try Dismiss tip again."; if the tip check fails, `src/App.svelte` shows a Retry alert.

## How it works
- `App.svelte` owns tip state (`loadFirstNoteTip`, `dismissTip`, `tipSaving`, `tipSaveFailed`, `tipLoadFailed`), reloads it on `sessionsChanged`, ignores stale responses via `tipRequest`, and clears the tip only after `dismissFirstNoteTip` succeeds. It passes `tipSaving` and `tipSaveFailed` through `src/lib/coach-context.ts`.
- AS6 store behavior (`src-tauri/src/store/sessions.rs`): `first_note_tip_session` returns nothing once the flag is `"1"`. The first completed real note is pinned in `first_note_tip_session` and kept even if other sessions finish out of order; `first_completed_note` is read before the update, so a note already completed earlier wins.
- `commands.rs` `read_onboarding_state` is extracted so tests can call it directly.
- AS10 (`src-tauri/src/diag/telemetry.rs`): `report` delegates to `report_with_sender`, so a test can inject a stub sender. `diag/log.rs` adds `TEST_LOG_LOCK` to serialize tests on the global logger.

## Config and data
- `first_note_tip_dismissed` (`"1"`, `store/settings.rs`) via `first_note_tip_dismissed()` and `dismiss_first_note_tip()`; `first_note_tip_session` holds the pinned session id. Exposed as `OnboardingState.firstNoteTipDismissed`. No migration.

## How to test
- `npm test` (`src/onboarding.svelte.test.ts`, `note-styles.svelte.test.ts`, `summary-panel.test.ts`): AS6 tip/sample/relaunch/recording, AS7 card with zero vs one style and the gate.
- `cargo test --lib --features fake-engines` and `cargo test --test store`: AS6 first-note pinning and dismissal persistence; `as10_onboarding_failures_leave_session_content_private_with_telemetry_off` seeds sentinel transcript, note and audio, fails settings reads, and asserts no sentinel in state, logs, error payloads or errors, no report sent, no pending telemetry, and session rows intact.

## Known limits
- The tip sits next to the StylePicker because the note-first tabs (recording-and-session PR3) have not landed; move it when they do.
- AS9 (first microphone prompt timing, and live-mic note generation showing the tip) is unverified. It needs a human on a Mac with fresh mic permission and a clean profile.
