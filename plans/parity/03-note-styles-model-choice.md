# Note styles and summary-model choice — Tauri parity spec
Status: draft for cwf; Date: 2026-11-03

## Intent (why users need it; who)
Scribular's users are therapists. The summary prompt is the product: each modality (DORA, Narrative, EFT, Gottman, IFS, Brainspotting, Couples) wants different note structure, and many clinicians have their own house format. Electron calls prompts "note styles". Users create their own, favourite the few they use, pick one before recording, and re-run a finished session with a different one.

Tauri today seeds the 7 built-in prompts but offers a bare `<select>` in Settings and no way to create, edit, favourite or delete. A new user on Tauri cannot make a note in their own format, which is the main reason to prefer Scribular over a generic tool.

Second need: a clinician on an 8 GB Mac wants the light model, and one on a 32 GB Mac may want a stronger one. Electron offers 3 models. Tauri ships exactly one hard-coded model.

## Electron behaviour today (with file:line refs)

### Data model
- A style is `{id, name, description, text, sampleOutput, isDefault, starred}`. 7 built-ins are in `src/store/defaults.js:7-71`. All have `sampleOutput: null`. Only `default-therapy-dora` is starred (`defaults.js:15`); the other six are `starred: false`.
- `activePromptId` (default `"default-therapy-dora"`, `defaults.js:72`) is the style used for the next recording. It is the only "default style" concept.
- `currentPrompt: ""` (`defaults.js:75`) is a legacy single-prompt field. `migratePrompts` (`store/store.js:25-58`) turns it into a custom starred style on first run, then deletes it. Not needed in Tauri (fresh start).
- Storage is one array under `prompts`. `updateConfig({prompts})` replaces the whole array (`store.js:119`), so there is no server-side validation of any kind.
- Resolution helpers: `getActivePrompt` falls back to `prompts[0]` (`store.js:160-162`); `getPromptById` (`store.js:166-168`); `getStarredPrompts` falls back to all prompts if none starred (`store.js:170-175`).
- Because electron-store only applies `defaults` for missing keys, new built-ins added in later releases never reach existing users. Electron has no reset or restore for built-ins (grep for reset/restore in `settings.js`/`store.js` finds nothing relevant).

### Editor (`src/renderer/views/settings.js`)
- List is grouped "Custom" (with a create button) then "Built-in" (`:205-248`). Each row: star button, name, description, a "Built-in" badge (`:140-200`). Clicking a row expands an accordion containing the shared editor. Clicking the open row collapses it (`:253-260`).
- Built-ins are read-only: name and text inputs are disabled, Save and Delete are hidden, and a Duplicate button shows instead (`:270-312`).
- Create: a new style named "New Custom Style" with empty text, `starred: true`. It is pushed optimistically, selected, then saved. On save failure it is popped (`:352-378`).
- Duplicate: copies a built-in as `"<name> (Copy)"`, custom, starred, `sampleOutput: null` (`:383-412`).
- Save: name is trimmed and falls back to `"Untitled Style"`; text is saved as typed, with **no empty check** (`:417-434`). Description is not editable (the description field is hidden at `:276-279`; `description` is never written by Save).
- Delete: custom only, native `confirm('Delete "<name>"? This cannot be undone.')`. If the deleted style was `activePromptId`, active becomes `prompts[0].id` (usually the first built-in), or `"default-therapy-dora"` (`:439-463`). Sessions that used it are untouched. The session view then shows "Deleted style" (`session.js:521-530`).
- Favourite: the star toggles `starred`. Unstarring the last starred style is blocked with a notification, "At least one style must remain starred." (`:326-347`). Unstarring does **not** change `activePromptId` (see bug below).
- Unsaved-change detection: `hasUnsavedChanges()` compares the inputs with the stored name and text (`:110-118`). It only enables or disables the Save button (`:123-125`, wired to `input` events at `:865-866`). There is no guard when the user collapses the row, selects another style, or leaves Settings. `loadSettings` re-selects the style from stored config (`:744-745`), which silently discards edits. This is a gap.
- "Try" CTA and `sampleOutput` panel: shown for custom styles with no sample (`:286-302`). Nothing ever writes `sampleOutput`, so it is dead UI.

### Choosing a style
- Recording screen: a dropdown lists starred styles, or all if none starred (`recording.js:35-65`). Picking one writes `activePromptId` immediately (`:112-114`). So the recording-screen choice is also the persisted default.
- The session record is saved with `promptId: config.activePromptId`, read when the recording stops (`recording.js:484`, `session.js:1566,1619`).
- Session view, regenerate: a second dropdown, same starred filter, defaults to the session's own `promptId` (`session.js:539-577`, called at `:826`). The pick lives in `sessionSelectedPromptId` only. It does not change `activePromptId`.
- Rewrite flow: the old summary, prompt id and model are stashed as `previousSummary/previousPromptId/previousModel` for undo, then `regenerateSummary({transcript, promptId, sessionId})` is sent (`session.js:1336-1386`). The main process resolves the style: use `currentPromptId` if it still exists, else the active style, else `defaults.prompts[0]` (`summarize.js:213-234`). The style used is saved onto the session when the summary completes (`summarize.js:301`).
- Provenance: the session view shows the style name, or "Deleted style" (`session.js:509-534`).

### Electron bugs and quirks (do not copy)
1. Unstarring the active style leaves `activePromptId` pointing at an unstarred style. The recording dropdown then shows `options[0]` (`recording.js:47-51`) but never saves it. The next session is summarized with the hidden active style, so what the user sees differs from what runs.
2. Deleting the only starred custom style can leave zero starred styles. The dropdown silently shows all.
3. Empty prompt text can be saved.
4. Edits are lost without warning on navigation.
5. A non-starred style a session used cannot be re-picked in the rewrite dropdown. The dropdown falls back to `options[0]` (`session.js:549`).

### Model choice
- Catalog in `src/common/models.js:9-38`: `gemma2:2b` (1.63 GB, default), `gemma3:4b` (3.34 GB, "better at multi-section formats like Narrative and Couples"), `gemma4:e2b` (7.16 GB, with a caution to use 16 GB RAM or more). `DEFAULT_MODEL_ID` is the first with `isDefault` (`:40-41`).
- The Settings panel is a radio list. It shows size, description, caution and an "Installed" badge from Ollama's tag list (`settings.js:594-672`; `ollama.js:178-187`; IPC `MODELS_GET_INSTALLED`, `registerIpcHandlers.js:363-366`, decorative and never throws).
- The pick is held as `pendingModelId`. Apply is enabled only when it differs from the saved model (`:677-690`). Apply is refused while recording or processing ("Finish or stop the current recording first.", `:704-711`). Then `MODEL_CONFIRM_SWITCH` shows a native dialog: "Switch to X? Scribular will restart [and download N GB]… Your sessions are not affected." (`registerIpcHandlers.js:368-392`). On confirm: save `model`, relaunch the app (`:717-722`). The download happens on the next startup.
- `MODEL_SET`/`MODEL_GET` (`api.js:34-48`) are legacy fire-and-forget channels that write or read the in-memory and stored model. The Settings panel does not use them (it calls `updateConfig({model})`). At run time `config.model || model` is used (`api.js:63-64`).
- Session records store `model`. The session view shows "· <catalog name>", or nothing for old sessions (`session.js:498-504`).
- Deleting a catalog entry later is handled by `getModelName` falling back to the raw tag (`models.js:58-61`).

## Tauri state today (what exists, what is missing, with file refs)

### Prompts
- Table `prompts(id, name, description, text, is_default, starred)`, seeded in migration v1 from `src-tauri/src/store/default_prompts.json`, with the active id stored as settings key `active_prompt_id` (`store/migrations.rs:44-91`). `MIGRATIONS` currently has v1 to v3 (`:11`).
- Rust API is read-only: `Store::list_prompts` (rowid order) and `get_prompt` (`store/prompts.rs:56-76`). `Prompt` has no `sample_output` (not needed, see Non-goals).
- IPC: `list_prompts` (`commands.rs:119-121`), `regenerate_summary(session_id, prompt_id)` (`commands.rs:~150-155`). Wrapper `listPrompts` and `regenerateSummary` in `src/lib/api.ts:48,71`. Nothing in `src/` calls `regenerateSummary`: there is no regenerate UI.
- Picker: a plain `<select>` bound to `settings.promptId` in `Settings.svelte:107-114`. It lists all prompts, has no star filter, and is not on a recording screen. `set_recording_settings` writes `active_prompt_id` (`store/settings.rs:64-83`).
- Per-session style: `start` copies `active_prompt_id` into `session.prompt_id` (`jobs/session.rs:250`), and `prompt_for` re-reads it when queuing (`:634-645`). `sessions.prompt_id` is plain TEXT with no FK (`migrations.rs:51`), so deleting a prompt cannot break session rows.
- Summarizer resolution: a missing prompt falls back to the active one, then fails (`jobs/summarize.rs:142-151`). `regenerate` rejects an unknown prompt id up front (`jobs/queue.rs:129-133`). The prompt id actually used is saved at completion (`summarize.rs:~256`).
- Tests: `tests/store.rs:257-285` asserts exactly 7 seeded prompts. Any change to seeding must keep it green or update it deliberately.

### Model
- `models/manifest.rs:37-66` pins 2 files: `whisper-base` (speech) and `gemma-2-2b-it-q4km` (LLM, 1,708,582,752 bytes, sha256 pinned, licence and licence URL). `ModelSpec` has `id, kind, display_name, file_name, url, size, sha256, license, license_url`. `ModelKind` is `Speech|Llm`.
- `llm/llama.rs` hard-codes `GEMMA_2_2B` in `model_id()`, `is_ready()` and `load()` (`:72-84`). It also hard-codes `CONTEXT_TOKENS = 8192` (`:29`, "gemma-2's trained context") and Ollama-matching sampling defaults for `gemma2:2b` (`:33`). The chat template comes from the GGUF itself (`:99`), which is what makes swapping models cheap.
- The queue caches the loaded model across jobs (`jobs/queue.rs:~282`, KTD5). Summaries record `model_id` from `loader.model_id()` (`summarize.rs:257`). `SessionRow.model_id` exists.
- `ModelStatus.svelte:7` hard-codes `SUMMARY_MODEL`; `FirstRun.svelte:8` hard-codes the speech model. `download_models` fetches every manifest entry (`commands.rs:134-146`), so adding LLMs to the manifest as-is would download all of them. This needs changing.
- No settings key for the chosen LLM exists.

## Goals / Non-goals / Anti-goals
Goals
- G1. Full note-style CRUD plus favourite and default, in Rust, with invariants the Electron app lacks.
- G2. Style picker where the user records and where they regenerate, both showing favourites.
- G3. A small curated summary-model list with in-app switching (no relaunch).

Non-goals
- N1. Migrating Electron styles. Fresh start; no import, no `currentPrompt` migration.
- N2. `sampleOutput` and the "Try" CTA. Dead in Electron; omit the field.
- N3. Editing built-ins. They stay read-only, with Duplicate. (Spec author's call: matches Electron, and keeps "restore" trivial.)
- N4. Per-session model choice, arbitrary user-supplied GGUF files, Ollama, or any model catalog fetched from the network.
- N5. Editing description in v1. Electron cannot either, but Duplicate copies it. Optional: allow description edit on custom styles; it costs one input.
- N6. Regenerate button, undo and "Deleted style" copy in the session view. Spec 09 owns those; this spec supplies the picker component and commands.

Anti-goals
- Do not copy Electron bugs 1 to 5 above.
- Do not hold a "pending" model in the backend. Selecting a model must not change anything until the user confirms.

## Requirements (numbered R1..)

### Note styles
- R1. A new migration (next free version at implementation time; 01 and 02 land first and take v4/v5) is a no-op for prompt data and exists only if a column is added. Required schema: add `prompts.updated_at INTEGER` only if needed for ordering; otherwise no schema change. Built-in sync (R9) runs on startup, not in a migration, because migrations are append-only and must never be edited after shipping (`migrations.rs:10`).
- R2. Rust `Store` gains `create_prompt(name, text) -> Prompt`, `duplicate_prompt(id) -> Prompt`, `update_prompt(id, name, text)`, `delete_prompt(id)`, `set_prompt_starred(id, bool)`. IDs are UUID v4 strings. If the repo has no uuid crate, use the id scheme `custom-<random hex from the OS RNG>` already available (see Dependencies).
- R3. New and duplicated styles are custom (`is_default=false`) and starred. Names are trimmed and fall back to "Untitled Style". Duplicate names `"<name> (Copy)"`.
- R4. `create_prompt` and `update_prompt` reject empty or whitespace-only text (`StoreError::EmptyPrompt`) and text over 8,000 characters. `update_prompt` and `delete_prompt` reject `is_default` rows with a typed `StoreError::BuiltInPrompt`. (Spec author's call: the longest built-in is about 2,000 characters, and summarize already fails on a prompt too long for the context, `summarize.rs:193`; 8,000 gives headroom without a token count at save time.) A new style is an unsaved editor draft until its first Save, which passes the non-empty text to `create_prompt`; no placeholder row is written.
- R5. Favourite invariant, enforced in one transaction in the store, not the UI: at least one style is starred at all times. Unstarring the last starred style returns `StoreError::LastFavourite` and the UI shows "At least one style must remain starred."
- R6. Default-style invariant: `active_prompt_id` always names an existing, starred style. Unstarring the active style moves active to the first remaining starred style in list order. Deleting the active style moves active to the first remaining style in list order and stars it if needed. This fixes Electron bugs 1 and 2.
- R7. Setting the default: `set_active_prompt(id)` rejects unknown ids; it auto-stars the target if unstarred (so the invariant holds). This replaces writing `promptId` through `set_recording_settings` from the prompt picker, though the existing `RecordingSettings.prompt_id` field stays and must remain consistent (see Design notes).
- R8. Delete never touches sessions. `sessions.prompt_id` is kept as is. A queued job whose prompt was deleted already falls back to the active one (`summarize.rs:142-151`), which is kept. A regenerate request for an unknown prompt keeps failing with `PromptNotFound` (`queue.rs:131`).
- R9. Restore of built-ins. On every start, `Store::sync_builtins()` upserts the rows of `default_prompts.json`: insert missing ids (starred=false, appended at the end of the built-in group); for existing built-in ids overwrite `name`, `description`, `text` and keep `starred`. Custom rows are never touched. Because built-ins cannot be deleted or edited, this is the whole "reset or restore" feature; there is no reset button. (Spec author's call: Electron has none and, per its store behaviour, never delivers new built-ins to existing users. Tauri gets updated built-ins for free.) The seed in v1 stays untouched, so `tests/store.rs:257-285` stays valid for a fresh DB.
- R10. List order is rowid within two groups: Custom then Built-in, as in Electron `settings.js:215-216`. Expose the grouping in the UI only; the store keeps rowid order.
- R11. Choosing a style per session:
  - The recording screen has a style picker listing starred styles (all, if none are starred, which R5 prevents). Choosing writes the default via `set_active_prompt`, so it persists, as in Electron.
  - The session's `prompt_id` is the active id at **Start** time (Electron reads it at Stop). (Tauri today copies it at Start, `session.rs:250`. Spec author's call: keep Tauri's Start-time behaviour, because it is more predictable, and because Electron reads it at Stop only by accident of code structure. Add a test that changing the picker mid-recording does not alter the running session.)
  - Regenerate uses a picker defaulting to the session's own style, and lists favourites plus the session's own style even if unstarred. This fixes Electron quirk 5. Choosing does not change the default.
  - The prompt used is recorded on the session when the summary completes (existing behaviour, `summarize.rs:256`).
- R12. Editor UI (Settings, new "Note styles" section): grouped list with star, name, description and a "Built-in" badge. Click expands an inline editor (name, text). Built-ins show text read-only and a "Duplicate to edit" button. "New style" creates a draft that is saved on first Save. It must not write an empty-text row. (Spec author's call: cleaner than Electron's optimistic empty row, `settings.js:352-378`.) Delete uses an in-app confirm with the style name, "This cannot be undone." A "Default" marker shows next to the active style with a "Make default" action.
- R13. Unsaved changes. The editor tracks a dirty flag (name or text differs from saved). While dirty: Save is enabled and an "Unsaved changes" label shows. Collapsing, selecting another style, leaving Settings, or closing the window while dirty asks "Discard changes?" with Save, Discard and Cancel. Save failure keeps the draft and shows the error.
- R14. Commands (in `commands.rs`, wrappers in `src/lib/api.ts`, ts-rs types exported): `create_prompt`, `duplicate_prompt`, `update_prompt`, `delete_prompt`, `set_prompt_starred`, and `set_active_prompt`. All return the updated prompt list, including the active id, so the UI never merges client-side. Errors are typed strings the UI can map to messages.

### Model choice
- R15. Decision: bring model choice over, as a curated list of **2 models**: keep `gemma-2-2b-it-q4km` as the default and add one stronger Gemma GGUF (Gemma 3 4B Instruct Q4_K_M is the direct analogue of Electron's `gemma3:4b`). The third Electron entry (Gemma 4 E2B, 7.16 GB) is deferred: it needs a llama.cpp build that supports it, and its memory caution makes it a poor default offer. Revisit after the bench in step 2 of Design notes. Electron describes the 4B model as stronger with multi-section formats (`models.js:21-24`), and 2 of the 7 built-ins are multi-section.
- R16. Manifest: add each added LLM as a `ModelSpec` with pinned commit URL, size, sha256, licence and licence URL, exactly as the existing pins. Values must come from the upstream file at a fixed commit and be recorded in the dependency-review doc. Do not invent hashes or sizes; Gemma 3 4B Q4_K_M is roughly 2.5 GB, which must be measured.
- R17. Downloads become opt-in per LLM. `download_models` keeps downloading speech plus the **selected** LLM only. Reuse spec 02's `download_model(id)` to fetch one entry. Unselected LLMs are not downloaded at first run, so first-run time and disk stay as today.
- R18. Selection is stored as settings key `summary_model_id`, default `gemma-2-2b-it-q4km`. New commands: `list_summary_models` (specs + `selected` + `state` from `ModelManager.readiness`), `set_summary_model(id)`. The "Installed" badge is `ModelState::Ready`, so there is no Ollama-style listing call. `MODELS_GET_INSTALLED` has no equivalent need.
- R19. Switch flow (replaces Electron's confirm-and-relaunch): user picks a model, then presses "Use this model". If it is not downloaded, the UI shows its size and licence, and a confirm says "Downloads N GB once." On confirm: save the key, start the download, and show progress using the existing `models://progress` event. Summaries run on the new model once it is `Ready`; until then, sessions wait in `AwaitingModel` (existing behaviour, `queue.rs:178-181`). If download fails, selection reverts to the previous model only when that model is ready; the UI reports the failure. Download cancellation is not supported.
- R20. No relaunch. The queue snapshots the selected `model_id`, passes it into `SummarizerLoader::load(model_id)`, and records that same id on the session. The queue drops its cached model when the selected id changes. Switching is refused atomically with a clear message while a summary is running or a recording is active (same rule as Electron `settings.js:708-711`); queued-but-not-started jobs use the new selection.
- R21. Per-model parameters. `CONTEXT_TOKENS` and the sampling defaults in `llama.rs:29-33` become per-model fields (context cap, stop tokens, temperature) so the 4B model is not given gemma-2 numbers. The chat template stays GGUF-provided.
- R22. The model name is saved on each session (`model_id` already exists). The session view shows "· <display name>"; nothing for none. Display names come from the manifest, falling back to the raw id.
- R23. Settings shows each model's size, one-line description, a caution for large memory use, licence link, and an Installed or Downloading badge. Descriptions and cautions are plain strings in the manifest. The 4B caution: "Needs about 8 GB of free memory. Slower than the default." Verify the number in the bench.

## Acceptance scenarios (numbered AS1..; Given/When/Then; each tagged automated or manual real-app check)

### Note styles
- AS1 (automated, cargo test `tests/prompts.rs`). Given a fresh DB, When `create_prompt("Mine","text")`, Then it exists with `is_default=false`, `starred=true`, and `list_prompts` returns 8 rows with the new one last.
- AS2 (automated). Given a built-in id, When `update_prompt` or `delete_prompt`, Then `BuiltInPrompt` is returned and the row is unchanged.
- AS3 (automated). Given a custom style, When `update_prompt` with whitespace-only text, Then `EmptyPrompt`. With a name of "  ", Then the stored name is "Untitled Style".
- AS4 (automated). Given only `default-therapy-dora` starred, When it is unstarred, Then `LastFavourite` and no change.
- AS5 (automated). Given active = custom style X (starred) and another starred style Y, When X is unstarred, Then active becomes Y in the same call. (Electron bug 1.)
- AS6 (automated). Given active = custom style X, the only custom style, When X is deleted, Then active is the first starred remaining style, at least one style is starred, and sessions whose `prompt_id` was X are unchanged.
- AS7 (automated). Given a built-in whose text was changed in the DB, When `sync_builtins()` runs, Then its text is restored and its `starred` flag is preserved. Given a new id in the embedded JSON, Then it is inserted unstarred. Custom rows are byte-identical. Running it twice changes nothing.
- AS8 (automated, fake-engines). Given a session with prompt X and its summary done, When X is deleted and the session is regenerated with X's id, Then the call fails with `PromptNotFound`. When a queued job holding X is run after X is deleted, Then it uses the active style and the session ends with the active style's id. (Existing behaviour at `summarize.rs:142-151`; add the test.)
- AS9 (automated). Given the picker changes active while a recording is running, When the recording stops, Then the session's `prompt_id` is the style active at Start.
- AS10 (automated, vitest). Given the editor with a dirty draft, When the user selects another style, Then the discard dialog shows. Save keeps the edits, Discard drops them, Cancel stays. Without edits no dialog appears.
- AS11 (manual real-app). Given a fresh install, When the user opens Settings, Note styles, creates a style, types a prompt, saves, stars it and makes it default, then records 30 seconds and stops, Then the session summary follows that style and the session shows its name.
- AS12 (manual real-app). Given a finished session, When the user opens the regenerate picker, Then it lists favourites plus the session's own style. After picking a built-in Narrative style and regenerating, Then the new summary has the three labelled sections and the default style is unchanged. (Regenerate button wiring is spec 09; this scenario is run once both land. In 03, verify the backend half automatically; see Coordination notes.)
- AS13 (manual real-app). Given a dirty editor, When the user closes the window, Then a discard prompt appears and Cancel keeps the window open.

### Model choice
- AS14 (automated). Given the manifest, Then LLM ids are unique, every LLM entry passes the existing pin checks (`manifest.rs:80-91`), and exactly one entry is the default.
- AS15 (automated, fake-engines). Given a fake loader and `set_summary_model` to a Ready model, When the next job runs, Then the loader is asked for the new model id, the cached model is dropped once, and the session's `model_id` is the new id.
- AS16 (automated, fake-engines). Given `set_summary_model` to a model that is not downloaded, Then the job waits in `AwaitingModel`. When the fake download fails, Then the selection reverts to the last Ready model and an error is surfaced.
- AS17 (automated). Given a summary job is `Summarizing`, When `set_summary_model` is called, Then it returns a typed "busy" error and the key is unchanged.
- AS18 (automated). First run downloads speech plus the selected LLM only; the second LLM's file is not requested.
- AS19 (automated, vitest). The model list renders size, caution, licence link, and the Installed badge from `ModelState`; "Use this model" is disabled when the selected model equals the saved one.
- AS20 (manual real-app, Apple Silicon). Given the default model installed, When the user selects the 4B model and confirms the download, Then progress shows, and when finished a new recording is summarized by it and the session view names it. Quit and reopen: the selection persists, and no re-download happens.
- AS21 (manual real-app). Pull the network during the 4B download. Then the UI shows the failure, the default model still works, and Retry resumes or restarts the download.

## Design notes (modules, commands/events, data model + migration, UI components)

Implementation order (the first step is the dependency review):
1. Dependency review (see below). Also pick the 4B GGUF source, commit, size and hash, and write them into `plans/DEPENDENCY_REVIEW_03_note_styles_model_choice.md`. Get Patrick's approval.
2. Bench the 4B model with `src/bin/bench.rs` on the 7 built-in styles. Check context, memory and quality. If it fails, ship styles only and keep the model list at 1 (the code path still lands, since R17 to R21 are generic).
3. Store: prompt mutations and invariants in `store/prompts.rs`, `sync_builtins`, new error variants in `store/mod.rs`, tests in `tests/prompts.rs`.
4. Commands and api.ts wrappers; register the commands in `lib.rs`; export types.
5. Note-style editor UI plus picker component.
6. Model catalog: manifest, `summary_model_id` setting, loader parameterisation, queue invalidation, commands, Settings UI.

Modules and files that change
- `src-tauri/src/store/prompts.rs` (mutations, `sync_builtins`), `store/mod.rs` (`StoreError` variants, re-exports), `store/settings.rs` (keep `RecordingSettings.prompt_id` consistent through the shared active-style setter; see below).
- `src-tauri/src/lib.rs` (call `sync_builtins` after migrations; register commands).
- `src-tauri/src/commands.rs` (new commands, ts-rs export of any new type).
- `src-tauri/src/models/manifest.rs` (second LLM entry; description and caution fields: add `description: String` and `caution: Option<String>` to `ModelSpec`, which is a ts-rs type, so regenerate `src/lib/types/ModelSpec.ts`), `commands.rs` (reuse the existing `download_model(id)` command) and `models/mod.rs` (`ensure_one(id)`).
- `src-tauri/src/llm/llama.rs`, `llm/mod.rs` (loader takes the selected id from the store or a shared handle; per-model params), `jobs/queue.rs` (drop the cached model on change; busy check), `jobs/summarize.rs` (no change expected beyond the model id).
- `src/lib/api.ts`, `src/lib/components/Settings.svelte` (split: keep recording settings; add `NoteStyles.svelte`, `NoteStyleEditor.svelte`, `StylePicker.svelte`, `SummaryModel.svelte`), `ModelStatus.svelte` (stop hard-coding `SUMMARY_MODEL`), `FirstRun.svelte`.
- Tests: new `tests/prompts.rs`, extend `tests/summarize_job.rs`, `tests/awaiting_model.rs`, `tests/llama.rs`; vitest for the editor dirty flag and the model list.

Data model
- Prompts: no required schema change. The settings table gets one new key, `summary_model_id`. No migration is needed for either (the settings table is a key-value store), so no migration is required by this spec. If the implementer finds a column is needed, append the next free version at the end of `MIGRATIONS` and never edit shipped entries.
- `RecordingSettings.prompt_id` duplicates the active prompt id (`settings.rs:22`). To avoid two writers, `set_active_prompt` and `set_recording_settings` use the same validated setter: reject unknown ids and auto-star an unstarred target. The Settings radio and prompt `<select>` are replaced by `StylePicker`.

Events
- Reuse `models://progress` for downloads. Emit the selected model id on `summary-model://changed` after a switch or rollback, including a switch to an installed model. Prompt changes need no event: each command returns the fresh list.

UI notes
- `StylePicker` takes `{prompts, selectedId, includeId?}` and is used on the recording screen and (with spec 09) the session view. The recording screen itself belongs to the existing app shell; this spec only adds the component and mounts it where `Settings.svelte` shows the select today, plus the recording screen if present at implementation time.
- Accessibility: the model list is a `radiogroup` with keyboard selection (Electron's rows had `role=radio`, `tabindex=0`, Space/Enter, `settings.js:613-669`); the star button has an `aria-label` and a pressed state.

## New dependencies (candidates + review needed) or "none"
Policy: the first step of implementation is a `plans/DEPENDENCY_REVIEW_*` doc and Patrick's approval, as in `DEPENDENCY_REVIEW_M3.md`.
- Styles: none expected. For ids, prefer the standard library plus an OS-RNG source already in the tree (check `Cargo.lock` for `uuid` or `getrandom` before adding anything). If neither is present, build the id from SQLite: `lower(hex(randomblob(16)))`, which needs no crate. In-house alternative is already this one.
- Model choice: **no new crates**. New *data* only: one more pinned GGUF (URL at a fixed commit, sha256, size, licence). The review doc must record that data source, its licence terms (Gemma Terms of Use, as for the current model) and whether the licence text must be shown at download. `ModelSpec` already carries `license` and `license_url`.
- Note: if llama-cpp-2 needs a version bump to support the chosen 4B model's architecture, that is a dependency change and goes through review. Gemma 3 support in the pinned version has not been checked.

## Out of scope
- Importing Electron styles or settings; Electron's `currentPrompt` migration.
- Sharing, exporting or importing styles as files.
- Style folders, tags, reordering, search.
- A third, larger model (Gemma 4 E2B) and any model that needs more than 16 GB.
- Per-session model choice and user-supplied GGUF files.
- Session-view regenerate button, undo and "Deleted style" wording (spec 09).
- Prompt text itself: the built-ins are Electron's, word for word; do not edit them in this spec.

## Depends on / interacts with (other parity specs by slug)
- 09-session-management-recovery: owns the regenerate button, undo (`previousPromptId`/`previousModel`) and provenance display. This spec delivers `StylePicker`, `regenerate_summary` semantics (R8, R11) and the model name label (R22).
- 01-encryption-audio-retention: moves the store to SQLCipher. The prompt tables are in the same DB; prompts hold no PHI, but R9's `sync_builtins` must run after the key unlocks the DB.
- 07-onboarding-samples: onboarding may default or explain a style. It should call `set_active_prompt`, not write the setting directly. Sample sessions reference built-in ids.
- 05-auto-update: new built-ins reach users through updates via R9 `sync_builtins`. Release notes should list them.
- 08-app-shell-tray-notifications: window-close guard for R13 uses whatever close-request hook that spec adds; if it lands later, R13 falls back to guarding navigation only.

## Coordination notes (gap-check 2026-11-04)
- R11: Start-time capture is the rule (AS9 tests it); the old "Stop time" wording was a contradiction and was fixed.
- R17: spec 02 lands first and adds the generic `download_model(id)` command, the `optional` flag on `ModelSpec` and `ensure_one(id)`. Reuse them; do not add a second per-model download path. If 02 already added `description`/`caution`-like fields to `ModelSpec`, extend rather than duplicate.
- AS12: until 09 lands there is no session detail view. Verify the backend part in 03 (automated: `regenerate_summary(session_id, prompt_id)` uses the chosen style and leaves the default unchanged). Run the UI part in 09.
- `StylePicker` for regenerate and the provenance/model label mount on the current-session result view (`RecordView`/`SummaryPanel`) until 09 re-mounts them in session detail.
- Settings: add a headed "Note styles" section to the existing Settings view. Spec 08 later restructures Settings into categories (note styles / session / general / about) and moves the section; keep it a self-contained component.
- 03 owns no-relaunch model switching (R20). Spec 05 must not add a relaunch path for model changes.

## Decisions for Patrick (with recommended default)
1. Ship a model choice at all? Recommended: yes, 2 models (the current default and Gemma 3 4B), gated on the step-2 bench. Fallback: styles only, model list stays at 1.
2. Switch behaviour: no relaunch, background download, revert on failure (R19 and R20). Recommended: yes. Electron needs a restart only because Ollama loads at startup.
3. Third model (Gemma 4 E2B, 7.16 GB)? Recommended: not now, for the reasons in R15.
4. Built-ins stay read-only with Duplicate, and updated built-ins overwrite on app update (R9). Recommended: yes. The cost is that a built-in text change in a release changes future summaries for users of that style. If this is a concern for clinical wording, make updates apply to new built-in ids only.
5. Can the recording-screen style change set the persistent default (as in Electron)? Recommended: yes, same as Electron.

## Size (S/M/L, rough days) and suggested PR split if > ~1 PR
Overall M to L, about 6 to 8 days.
- PR 1 (S, 1 day): dependency review doc and bench of the 4B model. No code in the app.
- PR 2 (M, 2 to 3 days): note styles: store, invariants, `sync_builtins`, commands, tests. No UI.
- PR 3 (M, 2 days): note-style editor UI, picker, dirty-state guard.
- PR 4 (M, 2 to 3 days): model catalog, `summary_model_id`, loader and queue changes, Settings UI. Skippable if the bench in PR 1 fails.
