# Onboarding tour and sample session (parity spec 07, PR #17)

## What it does
- First launch shows a skippable, non-modal four-step tour: Welcome, audio source (Settings), Sessions, note template (Settings). The Sessions step is dropped if its anchor is not on screen.
- Skipping or finishing marks onboarding complete. Settings has a **Show tour** button that restarts at step 1.
- First-run also asks for the telemetry choice (`FirstRun.svelte`).
- One example session (a labelled therapy-style transcript and summary, 1,245 s, dated one day back) is seeded once. Regenerating the summary of a sample is rejected.

## How it works
- Frontend: `src/lib/components/OnboardingTour.svelte` positions the card from `[data-tour="..."]` anchors (`input-mode`, `sessions`, `settings`), recomputes on scroll, clamps with the card's `offsetHeight`. `App.svelte` keys the tour by `tourRun` so Show tour remounts it at step 1. `SampleBadge.svelte` and `SampleBanner.svelte` exist but are not yet shown in a list or detail view.
- Backend: commands `get_onboarding_state`, `complete_onboarding`, `restore_sample_sessions` (`src-tauri/src/commands.rs`). `store/samples.rs` seeds in one transaction.

## Config and data
- Settings keys: `onboarding_completed` (`store/settings.rs`) and `SAMPLES_SEEDED` (`"1"` once seeded, so a deleted sample is not re-added).
- Migration v6 adds `sessions.is_sample INTEGER NOT NULL DEFAULT 0`; existing sessions stay non-samples.
- Restore: `restore_samples` deletes all `is_sample = 1` rows and inserts a fresh one.

## How to test
- `npm run check && npm test && npm run build`
- `cargo test --lib --features fake-engines` (set `TAURI_CONFIG='{"bundle":{"macOS":{"frameworks":[]}}}'` if the frameworks bundle is unavailable)
- Native: see CLAUDE.md "E2E / uitest"; pass `--app` the `.app` bundle path, not the `release` directory.

## Known limits
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
