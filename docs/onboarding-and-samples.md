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
