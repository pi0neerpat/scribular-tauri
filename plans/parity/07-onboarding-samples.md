# Onboarding and sample sessions — Tauri parity spec
Status: draft for cwf; Date: 2026-11-03


> **Dependency policy (Patrick, 2026-11-05; supersedes every dependency-review, approval, audit, pinning or vetting requirement in this spec and in `plans/DEPENDENCY_REVIEW_*.md`).** The only requirement for adding a crate or npm package: it is commonly used and actively maintained. No review doc, no approval step, no PR write-up. Prefer dependencies already in the tree. Separate from this: a pinned model file still needs its URL, size and sha256 taken from the upstream file, never invented.

## Intent (why users need it; who)
A new user (therapist or note-taker, not a developer) opens Scribular for the first time. They need three things quickly:
1. To get to a first recording without confusion about macOS permissions or model downloads (already the Tauri U10 job).
2. A short orientation: where to pick an audio source, where finished sessions appear, where the note template lives.
3. Something in the session list that shows what a finished session looks like, so the empty app is not a blank page.

Existing Electron users are not migrated (BRIEF). This spec is only for fresh Tauri installs.

## Electron behaviour today (with file:line refs)
**Onboarding is a 5-step skippable tour, not a setup wizard.** It has no input-mode, target-app, permission or model choices. Those choices are not part of Electron onboarding at all, so there is nothing of that kind to merge. The merge is about the tour copy and the completion flag.
- Steps (`src/renderer/views/onboarding.html`): 1 Welcome, "Let's take a quick tour." (:41-44); 2 "Click to enable an audio source — Use your microphone, application audio, or both." Spotlights `main-content-area` (:54-59); 3 "View Sessions — After you hit "finish" in a session, the transcript and summary will appear here." Spotlights `sidebar-sessions-section` (:70-75); 4 "Customize Your Notes — Edit your therapy note template in Settings." Spotlights `titlebar-settings-btn` (:86-91); 5 "All Systems Go — Need help? Reach out at support@scribular.com." with a "Get Started" button (:102-115).
- Step 2 navigates to the recording view (`onboarding.js:154-156`). Step 3 swaps its copy to "Your transcript and summary will show up here after you record. Everything stays on your device." when the sidebar has no sessions (`onboarding.js:157-165`).
- Spotlight: a cutout around the target element plus a 320px card placed beside it and clamped to the viewport (`onboarding.js:187-279`). For `main-content-area` the card always goes to the left.
- Close X, overlay click, Esc and "Get Started" all call `completeOnboarding()`, which sets `onboardingCompleted: true` (`onboarding.js:302-320, 366-411`). So skipping counts as completing.
- Gate: shown when `!config.onboardingCompleted` (`onboarding.js:342-350`), after a 500 ms delay, and never in E2E mode (`client.js:466-482`). Default `onboardingCompleted: false` (`store/defaults.js:83`); persisted via `updateConfig` (`store/store.js:108,137-138`).
- Re-open: a titlebar help button `#onboarding-help-btn` ("Show onboarding", `index.html:76`) calls `restartOnboarding()`, which reloads the sidebar and restarts at step 1. The code comment says it does not regenerate sample sessions (`onboarding.js:322-337`).
- Initial view (`renderer/views/initial.css`): a loading screen with app logo, a `#status-msg` area, and a `.model-unavailable-settings-btn` shown with the failure message when a model cannot be downloaded (:35-51). No steps, no permissions UI.

**Sample sessions are effectively dormant in production. Do not assume users see them.**
- Data: `getSampleSessions()` returns 3 sessions with ids `sample-session-1..3`, timestamps 1/2/3 days before "now", durations 1245/980/1120 s, and summary and transcript text starting with `[EXAMPLE]` (`store/defaults.js:84-109`). The `[EXAMPLE]` prefix is the only label. There is no `isSample` field.
- Seeding: `initializeSampleSessions()` adds them once, guarded by `sampleSessionsInitialized`; unpackaged builds get all 3, packaged builds only #1 (`store/sessions.js:123-159`). Summary and transcript are encrypted before storing.
- BUT it is only invoked when `process.argv` contains `--e2e-test` ("E2E tests only - skip for normal users", `sessions.js:161-169`). Onboarding is skipped in that same mode (`client.js:469`). Net effect: a real user never gets samples at first run.
- Restore: `reinitializeSampleSessions()` deletes every session whose id starts `sample-session-` and re-adds them with the same packaged/unpackaged rule (`sessions.js:354-397`). It is exposed on channel `SESSIONS_REINITIALIZE_SAMPLES = "sessions:reinitialize-samples"` (`common/ipcChannels.js:78`), handler `registerIpcHandlers.js:183-185`, preload `preload.js:154`. A grep of `src/` finds no renderer caller. It is unreachable from the UI.
- Unclear or buggy: the original intent (one `[EXAMPLE]` session for first-run) is visible in the packaged-vs-dev branch, but the wiring does not deliver it. Samples were also never given a visible badge.

**todo.md item** (`todo.md:14`): "Add the ability to include "what's new walkthroughs" when a new update is installed". Unchecked, no design. Electron today only shows an update-ready banner with a "See what's new" link to scribular.com/releases (`index.html:731`).

## Tauri state today (what exists, what is missing, with file refs)
- `src/lib/components/FirstRun.svelte` (46 lines): a checklist, rendered only while `!speechReady` (:30). Items: allow Microphone (button `openPrivacySettings('microphone')`), allow Screen & System Audio Recording (button `openPrivacySettings('systemAudio')`), then `SpeechModelStatus`. It calls `downloadModels()` on mount (:25) and exposes `speechReady` via `$bindable` (:9). Start recording is disabled until `speechReady` (`RecordView`, tested in `first-run.test.ts`).
- `PermissionNotice.svelte` (11 lines): `role="alert"` notice with an Open System Settings button, shown on a denied permission error (`deniedPrivacy` in `api.ts`).
- `ModelStatus.svelte` (35 lines): persistent summary-model (`gemma-2-2b-it-q4km`) status line with percent and Retry. `AwaitingModel` sessions keep their transcript and auto-summarize when the model is ready (`store/sessions.rs:12-22`; plan U10).
- `App.svelte:33-34` mounts `FirstRun` and `ModelStatus` above everything; there is no completion flag, no tour, no Help control, no sample content.
- Store: SQLite, migrations v1..v5 tracked by `PRAGMA user_version` (`store/migrations.rs:11`), a key/value `settings` table with `get_setting`/`set_setting` (`store/settings.rs:43-49`). `sessions` has no `is_sample` column (`migrations.rs:44-57`). `create_session` always inserts status `recording` (`sessions.rs:307-325`).
- Missing: onboarding copy and tour; `onboarding_completed`; sample seeding and restore; any Help entry point.

## Goals / Non-goals / Anti-goals
Goals
- Keep FirstRun as the gate for the first recording; bring Electron's orientation copy in as a non-blocking layer.
- Seed one clearly labelled sample session on first launch, deletable and restorable.
Non-goals
- No input-mode or target-app wizard step (Electron has none; `RecordView` already persists both via `RecordingSettings`).
- No migration of Electron `onboardingCompleted` or sample state (fresh start).
- No "what's new" walkthrough (see R12).
Anti-goals
- Onboarding must never add a click, a wait or a modal between launch and the Start recording button becoming enabled. If speech model download is the long pole, the tour fills that wait; it never extends it.
- Samples must never be counted as the user's data (never sent for summarization, never exported as if real, never included in telemetry counts).

## Requirements (numbered R1..)
R1. **No new dependencies expected.** If one is added, apply the dependency policy at the top of this spec.
R2. **Single first-run surface.** `FirstRun.svelte` remains the one place that handles permissions and the speech model. Preserve its permission/model gating and existing permission/model rows. This spec owns adding the telemetry-choice row (06 defines its setting and gate); the existing speech-model failure message and Retry action stay in `SpeechModelStatus`. Do not move these choices into a separate wizard. (Spec author's call: reuse over rewrite, the U10 tests already pin the gating.)
R3. **Tour as a layer.** Add `OnboardingTour.svelte`: 4 steps, non-modal, no full-screen overlay, no focus trap, so Start recording and the FirstRun buttons stay clickable the whole time. Steps adapt Electron's copy:
  1. Welcome ("Let's take a quick tour. Your permissions and speech model are getting ready below.")
  2. "Choose an audio source in Settings — Use your microphone, application audio, or both." Navigate to Settings for this step and anchor it to the existing input-mode controls there (`Settings.svelte`, `data-tour="input-mode"`).
  3. "View Sessions — After you finish a recording, the transcript and summary appear here." anchored to the session list. If the list is empty, use Electron's fallback copy "Your transcript and summary will show up here after you record. Everything stays on your device." (`onboarding.js:157-165`).
  4. "Customize Your Notes — Edit your note template in Settings." anchored to the Settings nav button. Say "note template", not "therapy note template", to match the note-style spec (03).
  Final card keeps "Need help? Reach out at support@scribular.com." with a Done button (Electron step 5, merged into card 4 so the tour is 4 cards, not 5). (Spec author's call: step 1 and 5 carry no information worth a click each.)
R4. **When it appears.** Immediately on first launch, concurrent with FirstRun and the model download (no 500 ms delay, no waiting for the download). Shown only if `onboarding_completed` is unset.
R5. **Completion.** Done, close X and Esc set `onboarding_completed = "1"`. Clicking outside does NOT complete (non-modal; a stray click on Start recording must not dismiss it silently). Skipping counts as completing, matching Electron (`onboarding.js:366-411`).
R6. **Re-open.** A Help control in the main nav ("Show tour") reopens the tour at step 1 without touching sessions (matches `restartOnboarding`, `onboarding.js:322-337`). Menu-bar item placement belongs to 08.
R7. **Persistence.** New settings keys `onboarding_completed` and `samples_seeded` via existing `set_setting`. New commands `get_onboarding_state` and `complete_onboarding` in `commands.rs`, wrappers in `api.ts`, types via ts-rs.
R8. **No-gate rule.** `speechReady` logic is untouched. A unit test asserts that rendering the tour with `speechReady=true` leaves Start recording enabled and with the tour open leaves it focusable.
R9. **Sample seeding.** On first launch, after migrations, if `samples_seeded` is unset: within one SQLite transaction, check that no row has `is_sample=1`, insert exactly one sample session if none exists (the content of Electron `sample-session-1`, `defaults.js:87-93`), then set `samples_seeded`. Seeding takes no model or network, and runs before the first window render (single insert, well under 50 ms). It runs in Rust `setup`, not inside a migration, so migrations stay schema-only. (Spec author's call: one sample, the packaged-build intent of `sessions.js:139-143`.)
R10. **Labelling.** New column `sessions.is_sample INTEGER NOT NULL DEFAULT 0` in migration v6. Sample rows have status `done`, `input_mode` `mic`, `title` "Sample session", `prompt_id` NULL, `model_id` NULL, no segments, no audio chunks, `created_at` = seeding time minus 1 day. `Session` and `SessionListItem` gain `isSample: boolean`. The list row shows a "Sample" badge; the detail view shows a banner "This is an example. It is not a real recording." Strip the `[EXAMPLE]` text prefix from the stored text (the badge replaces it). Do not identify samples by id prefix as Electron does (`sessions.js:364`); a column survives id changes and keeps queries cheap.
R11. **Sample behaviour.** Sample sessions can be deleted like any session. Regenerate summary is disabled with the tooltip "Sample sessions can't be regenerated" (no segments exist, and it would burn the model on fake text). Copy still works. Samples are excluded from any future usage counts (06) and from "export all" if 09 adds one.
R12. **Restore.** New command `restore_sample_sessions` (Rust: delete rows where `is_sample=1`, insert the sample again, one transaction, returns the new session id). Exposed in Settings as "Restore sample session". It replaces Electron's unreachable `SESSIONS_REINITIALIZE_SAMPLES` (`ipcChannels.js:78`). Deleting the sample does not auto-restore; Clear all (09) also removes it, and it comes back only through this button or a reset of the settings key.
R13. **What's new: out of scope.** Decision and reasoning below. Provide only the hook: store `last_seen_version` is NOT added here; 05 may add it.
R14. **Initial-view parity.** Electron's initial view model-failure button (`initial.css:35-51`, "open Settings") maps to the existing Retry action in `ModelStatus` and `SpeechModelStatus`, whose speech-model failure state already appears in FirstRun; no new model-failure UI is needed. Spec 06 owns the telemetry setting and gate; this spec owns placing its consent choice in FirstRun. Preserve the existing permission and model gating.

## Acceptance scenarios (numbered AS1..; Given/When/Then; each tagged automated or manual)
AS1 (automated, cargo test, in-memory Store). Given a fresh DB, when the store is opened and `seed_samples_if_needed` runs, then exactly one session with `is_sample=1`, status `done`, is returned by `list_sessions`, and `samples_seeded` is set. Running it again changes nothing.
AS2 (automated, cargo test). Given the sample was deleted, when `seed_samples_if_needed` runs on next launch, then no sample is re-created (flag is already set).
AS3 (automated, cargo test). Given the sample was deleted, when `restore_sample_sessions` runs, then one sample exists; running it twice still leaves exactly one.
AS4 (automated, cargo test). Given a v5 DB with sessions, when migration v6 adds `is_sample`, then existing rows have `is_sample=0` and nothing is lost.
AS5 (automated, cargo test + fake-engines). Given a sample session, when `regenerate_summary` is called, then it returns an error and no job is queued.
AS6 (automated, vitest). Given `onboarding_completed` unset, when `App` renders, then the tour step 1 is present; given it is "1", then no tour renders.
AS7 (automated, vitest). Given the tour is open and `speechReady=true`, then the Start recording button is not disabled and not covered (no overlay element with `position: fixed; inset: 0`).
AS8 (automated, vitest). Given step 3 and an empty session list, then the fallback copy "Everything stays on your device." renders; given a non-empty list it does not.
AS9 (automated, vitest). Given Esc or close X, then `complete_onboarding` is called once and the tour unmounts; given an outside click, then it does not.
AS10 (automated, vitest). The Help control reopens the tour at step 1 even if `onboarding_completed` is "1", and calls no session command.
AS11 (manual real-app, fresh macOS user, clean app-data dir). Given a first launch with the speech model still downloading, when the app opens, then the tour is visible, FirstRun shows both permission rows, a "Sample" badge is on the one list entry, and nothing blocks clicking the Open Microphone Settings button. Record timing from launch to Start recording enabled and compare it with the same run on the commit before this change; the difference must be inside run-to-run noise (no added step).
AS12 (manual real-app). Given the user finishes the tour, quits and relaunches, then no tour appears. Given they click Help, the tour appears. Given they delete the sample and click Restore sample session, it returns with the badge.
AS13 (manual real-app, run in spec 09). Given the sample is open, then the banner shows, Regenerate is disabled, and Copy works. (Before 09 there is no session list or detail view to open it from. In 07, AS5 covers the backend rejection and a vitest renders the banner component with `isSample=true`.)

## Design notes (modules, commands/events, data model + migration, UI components)
- Rust: `src-tauri/src/store/migrations.rs` append migration v6 (`ALTER TABLE sessions ADD COLUMN is_sample INTEGER NOT NULL DEFAULT 0`). New `src-tauri/src/store/samples.rs` holding `SAMPLE_TRANSCRIPT`, `SAMPLE_SUMMARY`, `Store::seed_samples_if_needed`, `Store::restore_samples`. Seeding checks `is_sample=1` and sets `samples_seeded` in the same transaction. Sample content lives in Rust constants, not a JSON file, mirroring `default_prompts.json` handling only if that is how `prompts::defaults()` does it (follow that precedent). `store/sessions.rs`: add `is_sample` to `Session`, `SessionListItem` and the row mappers; `regenerate_summary` in `commands.rs:149` rejects samples. `store/settings.rs`: key constants `ONBOARDING_COMPLETED`, `SAMPLES_SEEDED`.
- Commands (`commands.rs`, wrapped in `api.ts`, types exported through `export_ipc_types`): `get_onboarding_state() -> OnboardingState { completed: bool }`, `complete_onboarding()`, `restore_sample_sessions() -> String`. Seeding is called from `lib.rs` setup, not a command.
- Frontend: new `src/lib/components/OnboardingTour.svelte` (step list as data; anchors via `data-tour="input-mode|sessions|settings"` attributes added to Settings, session list and nav in `App.svelte`; step 2 navigates to Settings); a card positioned with CSS anchoring (no spotlight cutout; Spec author's call: the cutout needs `getBoundingClientRect` layout code, ~90 lines in Electron `onboarding.js:187-279`, for little value and it blocks clicks). Sample badge in the session list item component; banner in the session detail. `FirstRun.svelte` adds only the telemetry choice, not a tour subtitle; its existing speech-model failure state remains in `SpeechModelStatus`.
- Ordering with licensing (04): if a trial gate or licence step is added, it comes after the tour starts, never before Start recording is enabled. Interaction details are 04's to define.
- Tests: add `src-tauri/src/store/samples.rs` unit tests, extend `first-run.test.ts` with tour cases.

## New dependencies (candidates) or "none"
None. The tour is plain Svelte and CSS; the in-house alternative to a tour library (e.g. a Shepherd/Driver.js style package) is the ~100-line component above.

## Out of scope
- Input-mode/target-app wizard steps.
- Migrating Electron onboarding state or sample sessions.
- "What's new" walkthrough after an update (decision below).
- Update banner and release notes link (05). Native menu item for Help (08). Note template editor itself (03).
- Showing more than one sample, or localized samples.

## Depends on / interacts with (other parity specs by slug)
- 03-note-styles-model-choice: tour step 4 points to its editor; copy must stay accurate.
- 05-auto-update: owns the "See what's new" link (`index.html:731`) and any `last_seen_version` tracking.
- 09-session-management-recovery: Clear all and Delete must handle `is_sample`; samples need list-badge awareness and should not enter crash recovery.
- 01-encryption-audio-retention: if the DB is SQLCipher-encrypted, seeded rows are encrypted for free (Electron encrypts them by hand, `sessions.js:145-149`). Seeding must run after the DB key is opened.
- 04-licensing-trial, 06-diagnostics-telemetry (any first-run consent or notice must not add a step before Start recording; samples excluded from counts), 08-app-shell-tray-notifications (Help menu entry).

## Coordination notes (gap-check 2026-11-04)
- **No session list or detail view exists until 09** (`App.svelte:13` has only `record`/`settings`). 07 delivers `is_sample`, seeding, restore, the backend regenerate rejection (AS5) and stand-alone `SampleBadge`/`SampleBanner` components with vitest. 09 mounts them in the list row and detail view.
- **Tour step 3** (anchor `data-tour="sessions"`): steps are data. If the anchor is missing at render time, drop the step (the tour becomes 3 cards) and add a vitest for that. 09 adds the anchor and the step returns. AS8 covers the step only when the anchor exists.
- **AS11:** check the "Sample" badge only once 09 has landed. In 07, check the sample through `list_sessions` (devtools console) instead.
- **Seeding after unlock:** after 01, `Store::open` can be Locked. Run `seed_samples_if_needed` after a successful unlock and migrations, never on the Locked path. FirstRun and the tour render only after the lock check (01).
- **FirstRun consent and support link:** add 06's telemetry choice to FirstRun here (06 shipped the Settings toggle). Open the "support@scribular.com" mailto through 04's `open_external_link`. "Restore sample session" is a headed item in the current Settings view; 08 moves it into its category layout. The Help menu entry is 08.

## Decisions for Patrick (with recommended default)
1. Seed a sample session for real users at all? Electron's code only does it in E2E mode, so real users never saw it. Default: yes, one sample, labelled, deletable, restorable. Alternative: copy Electron exactly and ship no sample.
2. What's new walkthrough: Default: out of scope (decided below). Revisit when Tauri has a second release.
3. Tour replaces Electron's overlay and spotlight with a non-blocking card. Default: yes, because it keeps Start recording clickable while models download.

### What's new walkthrough (todo.md:14): decision
Noted, in scope for this spec? **No.** Reasons: (1) there is no installed base to show a "what's new" to; Tauri users are new and the first update is the first time this matters; (2) it depends on the update flow (05) for the trigger, and on a release-notes source that does not exist; (3) Electron has nothing to port, only a link to the website. Hook for later: `OnboardingTour` takes a step list as data, so a future walkthrough can reuse it, and 05 can store `last_seen_version` and compare it at launch.

## Size (S/M/L, rough days) and suggested PR split if > ~1 PR
M, about 3-4 days.
- PR 1 (~1.5 days): v6 `is_sample` migration, `samples.rs`, guarded seeding in setup, restore command, `is_sample` types, cargo tests (AS1-AS5).
- PR 2 (~2 days): `OnboardingTour.svelte`, anchors, Help control, completion commands, badge/banner, Settings restore button, vitest (AS6-AS10), manual checks (AS11-AS13).
