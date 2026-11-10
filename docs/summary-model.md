# Summary model (Gemma 4 E2B)

## What it does
- Gemma 4 E2B Instruct, Q4_K_M (`GEMMA_4_E2B`) is the only summary model and `DEFAULT_LLM` (`src-tauri/src/models/manifest.rs`). Settings no longer shows a summary-model picker.
- At startup, an obsolete or unknown saved `summary_model_id` is reset to `DEFAULT_LLM` (`restore_summary_model_setting`, `src-tauri/src/commands.rs`).

## How it works
- **Model pin** (`src-tauri/src/models/manifest.rs`): ID `gemma-4-e2b-it-q4km`, file `gemma-4-E2B-it-GGUF-Q4_K_M.gguf`, 3,427,879,936 bytes, pinned SHA-256, Apache-2.0. The manifest tests assert it is the only LLM and the old Gemma 2 ID is absent.
- **Download and verification** (`src-tauri/src/models/download.rs`): the existing `ModelManager` downloads or resumes a `.part` file, checks its size and SHA-256 against the manifest, then installs it. A `.sha256` stamp lets later launches verify without re-hashing when the stamp matches.
- **RAM gate** (`src-tauri/src/models/mod.rs`, `src-tauri/src/hardware.rs`): `MIN_SUMMARY_RAM_BYTES` is `8 << 30` (8 GiB); `ModelManager` compares it with `physical_memory_bytes()`. Below the threshold, summary availability is `Unsupported` and only the summary-model download is skipped; required speech-model downloads continue. Unknown RAM (`None`) passes the gate.
- **Unavailable summaries**: `SummaryAvailability` is `Ready`, `Downloading`, `Unsupported`, or `LoadFailed`. If the Mac is below the gate or Gemma 4 cannot load, summaries cannot run, while recording and transcription remain available. Existing summaries remain viewable. Settings can retry a load failure (`retry_summary_model`, `src-tauri/src/commands.rs`).
- **Thinking text** (`src-tauri/src/llm/llama.rs`): `format_gemma4_user_prompt` formats a Gemma 4 user turn without an explicit thinking block; `gemma_4_formats_a_user_turn_without_thinking` checks the template. There is no separate post-generation thinking-text filter.
- **Low-memory live transcript wait** (`TranscriptWaiting` in `src-tauri/src/jobs/mod.rs`; UI in `src/lib/components/LiveTranscript.svelte`): while a summary is running and Whisper must be unloaded, the live transcript shows “Live transcript will start when the current summary finishes.” A wait caused only by another source's Whisper load stays silent. The work landed in `1954f8a`, was corrected in `8f581af`, and its test timing was fixed in `ee0b1d9`.
- **Old Gemma 2 cleanup** (`src-tauri/src/models/mod.rs`, `src-tauri/src/llm/llama.rs`): `gemma-2-2b-it-Q4_K_M.gguf` is removed immediately on a below-threshold Mac. Otherwise it is kept until Gemma 4 loads and passes `smoke_test()`; download, verification, load, or smoke-test failure leaves the old file in place.

## Config and data
- The app stores model files in `appDataDir/models/`. On macOS this is `~/Library/Application Support/com.app.scribular.tauri/models/` (`src-tauri/src/lib.rs`). The Gemma 4 file is `gemma-4-E2B-it-GGUF-Q4_K_M.gguf`; download uses the adjacent `.part` file and `.sha256` stamp. The obsolete file is `gemma-2-2b-it-Q4_K_M.gguf`.
- UITests link the app's `models` directory to `~/Library/Caches/scribular-uitest/models/`; see `tools/uitest/README.md`.
- The persisted setting is `summary_model_id` (`src-tauri/src/store/settings.rs`). At startup, an ID not present in the manifest is replaced with `gemma-4-e2b-it-q4km`; the current code logs `resetting unknown summary model selection`. Existing session `model_id` values are retained as session history.

## How to test it
- Rust: `cargo test --manifest-path src-tauri/Cargo.toml --features fake-engines`. The fake-engine test setup exercises app/job behavior without loading the real Gemma weights; model manifest, RAM-gate, loader-state, and prompt-format checks are in Rust tests (`src-tauri/src/models/manifest.rs`, `src-tauri/src/models/mod.rs`, `src-tauri/src/llm/llama.rs`).
- Frontend: `npm test` and `npm run check`.
- Native UI: `tools/uitest/scenarios/as11-note-style-default.uitest` records with a newly saved default note style and checks the transcript and summary. Run it as documented in `tools/uitest/README.md`. The shared model cache means ordinary UITest runs reuse verified models; fresh-download UITests are reserved for download/onboarding coverage.
- Acceptance coverage and manual checks are listed as AS1–AS11 in `plans/2026-11-07-note-style-coach-spec.md`; Phase 1 scope is summarized in `plans/2026-11-08-note-style-coach-p1-build.md`.

## Known limits
- The 8 GiB threshold is provisional (`MIN_SUMMARY_RAM_BYTES`); it has not been replaced by complete real-machine measurements.
- PR #23 waived AS10's fresh-update download, old-model cleanup verification, and full measurements (peak RSS, load time, tokens/s, transcript phrases, and threshold rationale). The cleanup code is implemented, but that end-to-end path was not measured.
- AS10a remains unverified: simultaneous mic+system capture during the wait, exact-once notice show/clear behavior, and window-reload behavior were not confirmed on a real 8 GiB Mac (`plans/2026-11-07-note-style-coach-spec.md`, AS10a).
- AS11 found that Brainspotting, IFS, Gottman, Couples/Intimacy, and EFT introduced unsupported clinical details in one sample with Gemma 4. No built-in note styles were changed; retuning is a follow-up.
