# Multi-speaker diarization — Tauri parity spec
Status: draft for cwf; Date: 2026-11-03

## Approved execution decision — 2026-11-04

Patrick approved official `sherpa-onnx`/`sherpa-onnx-sys` `=1.13.8` and its official native release for implementation and integration without an Electron parity result. This supersedes R1 as a prerequisite to implementation and the historical FFI-first recommendation below. Cross-engine parity remains an open, unmeasured verification item: the Electron baseline and fixed-clip comparison are unavailable, and no accuracy or parity claim is made. This implementation approval is separate from verification, technical review, Patrick product approval, merge, and release/signing approval.

> Historical authoring note: third-party crate and build claims were unverified when this draft was written. The selected sherpa crate/native release were rechecked as recorded in `plans/DEPENDENCY_REVIEW_DIARIZATION.md`; the alternative-engine maturity notes below remain historical and unverified.

## Intent
Couples and family therapists record in-person sessions with 2–4+ voices on one mic. A plain transcript is useless to them, and `todo.md:10` calls diarization a gating feature for marriage and family therapists. Users need to: turn it on once, say how many people spoke when they stop, get a transcript split by speaker, name the speakers ("Therapist", "Maria"), and get a summary that uses those names. They can skip, and can add speakers later.

## Electron behaviour today (v1.5.10)
- **Setting**: `multiSpeaker: { enabled: false }` (`src/store/defaults.js:76-78`); read/written in `src/store/store.js:111,143`; toggle `multi-speaker-toggle` in `src/renderer/views/settings.js:773-775,914-927`.
- **Stop flow**: with the setting on, stop shows a speaker-count prompt, options 2 / 3 / 4+ (`src/renderer/views/recording.js:459-473,531-533`; 4+ sends `numSpeakers: null`). The tray replaces Stop with a "Stop - How many speakers?" submenu (`src/main/tray.js:33-79,137`).
- **Pipeline** (`src/service/audio/audioWorker.js`): `stopTranscription({numSpeakers, skipDiarization})` at :472. Gate: `!skipDiarization && workerData.multiSpeakerEnabled && DIARIZATION_MODEL_PATH` at :484. Whisper runs on the full WAV after stop, producing timestamped segments.
  - *Single source* (:575-631): `embedAndLabelSegments` with threshold 0.7 and `maxSpeakers: numSpeakers ?? null`. The comment at :613 says 0.5 "caused phantom speaker accumulation in sessions >20min".
  - *Two sources* (:485-574): diarize each side separately, `perSideMax = numSpeakers ? numSpeakers-1 : 4` (:490), threshold 0.7, `micOffset` = number of system speakers (:547), then `mergeLabeledSources` and `buildRawTranscriptAndMap` (`diarizeUtils.js:12,27`).
  - On any failure it falls back to the plain transcript (:561-564, :620-623). It posts `{diarizing:true}` and then `transcriptionComplete{plainTranscript, rawTranscript, speakerMap, isDiarized:true}`; `audio.js:425-428,445-461` forwards that as `TRANSCRIPT_READY`.
  - Non-diarized two-source output is the `Client:/You:` merge (`mergeDiarizedSegments`, :350; :634-642).
- **Algorithm** (`diarize.js:79-177`): sherpa-onnx-node `SpeakerEmbeddingExtractor` (cpu, 4 threads, created :95) embeds each whisper segment. Segments under 500 ms inherit the previous speaker (:114-119). Otherwise `manager.search({v, threshold})`: a hit reuses that speaker; a miss registers `SPEAKER_n` unless the cap is reached, in which case it takes the nearest speaker via `threshold: 0` (:130-141). The output `rawTranscript` strips `[bracketed]` tokens and merges consecutive same-speaker turns (:147-163). The default `speakerMap` is "Speaker N+1" (:165-173). This is greedy online matching, with no VAD and no segmentation model: whisper provides the segment boundaries.
  - `compute(stream, false)` is deliberate: external NAPI buffers are rejected in worker threads (:128).
  - Benchmarks (`plans/MULTI_SPEAKER_DIARIZATION.md`): 2 speakers 99.6%, 3 speakers ~86–89%, 4 speakers not production-ready, ~3 min for a 50-min recording on an M-series Mac.
- **Model**: only the embedding model `3dspeaker_speech_campplus_sv_zh_en_16k-common_advanced.onnx` (28,281,164 bytes, Apache-2.0), **bundled** under `src/service/runners/diarization-models/` and resolved by `src/utils/path.js:93-102`. No download step. `package.json:37` pins `sherpa-onnx-node` 1.12.31; `electron-builder.json:30` asarUnpacks it; the Electron plan records macOS rpath fixes for `libonnxruntime`.
- **Session data** (`src/store/sessions.js:189-254`): `rawTranscript`, `speakerMap` (encrypted JSON), `plainTranscript`, `isDiarized`, plus the derived `transcript`.
- **IPC** (`src/common/ipcChannels.js:117-120`, `preload.js:156-162`, `registerIpcHandlers.js:187-198`):
  - `confirmSpeakerLabels` (`sessions.js:404-425`) derives the transcript with `applySpeakerMap`, sets `isDiarized` true.
  - `skipDiarization` (`sessions.js:~432-449`) stores `plainTranscript` as the transcript, `isDiarized` false, and keeps the diarization data so "Add speakers" works later.
  - `updateSpeakerMap` (`sessions.js:456-476`) re-derives the transcript from the stored raw transcript.
  - `applySpeakerMap` (`diarize.js:186-213`) replaces only `^SPEAKER_\d+:` at line start and merges consecutive same-label lines.
- **Renderer flow** (`src/renderer/views/session.js`): the `TRANSCRIPT_READY` handler (:1490-1606) saves the session and **starts summarization immediately on `plainTranscript`**, with the Speakers card shown in parallel. Confirm calls `confirmSpeakerLabelsAndRegenerateSummary` (:969), which regenerates the summary with named labels. Skip leaves the plain transcript with an "Add speakers" bar (:1012). After confirm, a compact bar (`setupCompactBar`, :1148-1200) edits names with text substitution in the summary and no model re-run. Partial names autosave on blur (`setupSpeakersCard`, :1074-1140). **Unclear whether the substituted summary is persisted — verify before porting (see R13).**
- **Card UI** (`src/renderer/features/speakerAssignment.js`, `src/renderer/styles/speakers.css`): title "Who's speaking in this session?" (:181), sample quotes (`extractSampleQuotes` :138), preset chips Me→"You" and Client→"Client" (:329), 5 cycling colours (:6), coloured transcript (`renderColoredTranscript` :67), compact bar (:392).
- **Tests**: `test/wdio.diarization.conf.ts` (`--e2e-test --e2e-diarize`, fixtures from `buildE2eDiarizedFixture`, `audio.js:604-652`), `test/unit/applySpeakerMap.test.js`, `test/unit/sessions-diarization.test.js`, plan `plans/diarization-tests.md`.
- **Inconsistencies**: (1) `diarize.js` defaults threshold 0.5 but every caller passes 0.7 — port 0.7. (2) `plans/MULTI_SPEAKER_DIARIZATION.md` says telehealth is "gated out", but the worker implements two-source diarization — port the code. (3) `docs/DIARIZATION_OPTIONS.md` recommends TinyDiarize first; sherpa-onnx shipped instead — treat as stale. (4) `plans/DIARIZATION_STREAMING.md` (live labels) is deferred, not parity.

## Tauri state today
- **No diarization.** Sources are separate: `jobs/session.rs:76` maps Mic→[Mic], System→[System], MicAndSystem→[System, Mic].
- **Streaming**: `stt/stream.rs` transcribes 20 s windows with a 15 s hop and 5 s overlap, dedups, and returns segments in recording time per source. `Recorder::window` (`jobs/session.rs:507-547`) stores each via `store.add_segment(id, source, &segment)`. The `segments` table (`store/migrations.rs` v1) has no speaker column.
- **Finalize** (`jobs/session.rs:591`): drain → `transcript()` (:690-721; `merge_diarized_segments(System, Mic)` labels "Client"/"You" for MicAndSystem, plain join otherwise) → `clean_transcript` → `store.save_transcript` → summary queued via `jobs.regenerate`. Status machine in `jobs/state.rs`.
- **Audio retained**: 16 kHz mono i16 chunks in `audio_chunks`, read through `store.audio_samples(id, source)` (`store/sessions.rs:536`). Segment `start_ms/end_ms` are in the same recording-time axis, so audio slices line up.
- **Models**: `ModelManager` (`models/`) with pinned `manifest()` (`models/manifest.rs`: `ModelSpec{id,kind,display_name,file_name,url,size,sha256,license,license_url}`; Whisper base 147,951,465 B, Gemma 2 2B Q4_K_M 1,708,582,752 B). `ModelKind` is only `Speech | Llm`; `ensure_all` downloads every spec.
- **Settings**: KV `settings` table (`store/settings.rs`); `RecordingSettings{input_mode,target_app,prompt_id}`. No multi-speaker key.
- **IPC**: `commands.rs` (`stop_recording` takes no argument), ts-rs types, wrappers in `src/lib/api.ts`. Components: `RecordView`, `LiveTranscript`, `SummaryPanel`, `Settings`, `ModelStatus` under `src/lib/components/`.
- **Missing**: setting, count prompt, engine, model entry, schema, commands, assignment UI.

## Goals / Non-goals / Anti-goals
**Goals**: parity for single-mic (2 / 3 / 4+) and mic+system; same data semantics (raw transcript + speaker map, derived labelled transcript); confirm / skip / add-later / rename; summary uses names; fully on-device; any failure falls back to the plain transcript and never loses a session.

**Non-goals**: live speaker labels; better accuracy than Electron; fixing 4+ speakers; non-English tuning.

**Anti-goals**: no diarization while recording (CPU contention with streaming whisper); no blocking recording on a model download; no cloud or Python path; no speaker content in logs.

## Requirements
- **R1** Dependency review: `plans/DEPENDENCY_REVIEW_DIARIZATION.md`. The original cosine ≥ 0.99 fixed-clip comparison against `sherpa-onnx-node` remains open verification, but Patrick's dated approval above supersedes it as a prerequisite to implementation. Do not claim parity until the comparison is measured.
- **R2** Setting `multi_speaker` (bool, default false) in the KV store, with get/set commands and a Settings toggle.
- **R3** Add `ModelKind::Speaker` and a `speaker-campplus` manifest entry (28,281,164 B, Apache-2.0, sha256-verified). It is **optional**: skipped by `ensure_all`, downloaded on demand when the user enables the toggle; the toggle only stays on once the model is Ready.
- **R4** Engine behind a trait (`SpeakerEmbedder::embed(&[f32]) -> Result<Vec<f32>>`, loaded on demand like `TranscriberLoader`) with a fake under `fake-engines`.
- **R5** Pure-Rust labelling port of `embedAndLabelSegments`: greedy online match by cosine similarity, threshold 0.7, `min_segment_ms` 500 (short segments inherit the previous speaker, else `SPEAKER_0`), cap with nearest-speaker fallback, default map "Speaker N+1".
- **R6** `stop_recording` gains `num_speakers: Option<u8>` (`Some(2)`, `Some(3)`, `None` = 4+). The stop UI prompts when the setting is on and no count is supplied. Tray wiring is spec 08.
- **R7** Diarization runs in `Recorder::finalize` after `drain` and before the summary is queued, with progress message "Identifying speakers…". It reads stored segments and `audio_samples` per source.
- **R8** Single source: cap = `num_speakers`. Two sources: per-side cap `n-1` (4 when None), mic offset = number of distinct system speakers, merged by `start_ms` (Electron `audioWorker.js:490,547`).
- **R9** Persist on the session: `plain_transcript`, `raw_transcript`, `speaker_map` (JSON), `diarization_state` (`none | pending | confirmed | skipped`). `transcript` is the derived labelled text.
- **R10** The summary starts on `plain_transcript` immediately. On confirm it regenerates with the labelled transcript through the existing `regenerate_summary` path, skipped when the map equals the defaults.
- **R11** Commands `confirm_speaker_labels(session_id, speaker_map)`, `skip_diarization(session_id)`, `update_speaker_map(session_id, speaker_map)`. The raw transcript is read from the store, never sent by the UI. Each returns the derived transcript; an unknown session is an error.
- **R12** `apply_speaker_map` ports `diarize.js:186-213` exactly: only `SPEAKER_\d+:` at line start, consecutive same-label lines merged, unmapped IDs stay.
- **R13** After `update_speaker_map` the stored summary gets old→new name substitution (whole word, no LLM run). Whole-word matching is deliberate because Electron's plain replace would corrupt substrings (e.g. a speaker named "Mom" inside "Moment"). Verify first whether Electron persists the substituted summary.
- **R14** Missing model or any engine error → plain transcript, `diarization_state=none`, non-blocking notice. Recording and summary are unaffected.
- **R15** UI: count prompt, Speakers card (name input, sample quotes, "Me" and "Client" chips plus typed-name chips, Confirm, Skip, partial-name autosave on blur), compact "Edit speakers" / "Add speakers" bar, 5-colour transcript.
- **R16** Cancelling or deleting a session discards diarization data.
- **R17** Diarization honours the existing cancel path, checked per segment.

## Acceptance scenarios
- **AS1** (automated, cargo test) `apply_speaker_map`: swap, rename, merge consecutive same-label lines, mid-line "SPEAKER_1:" untouched, unmapped ID, empty map. Mirrors `applySpeakerMap.test.js`.
- **AS2** (automated, fake embedder) Two orthogonal voices alternating → `SPEAKER_0/1`, map "Speaker 1/2", same-speaker turns merged, `[MUSIC]` stripped.
- **AS3** (automated) Cap 2 with three distinct voices → third goes to the nearest; cap None allows 4. A segment under 500 ms inherits the previous speaker; a leading short one becomes `SPEAKER_0`.
- **AS4** (automated) Two-source merge, cap 3: system IDs 0..k, mic IDs offset by k, no collisions.
- **AS5** (automated, fake-engines + store) A recording with the setting on ends `pending` with `raw_transcript` set and the summary queued on `plain_transcript`; `confirm_speaker_labels` sets `confirmed`, derives the transcript, queues exactly one regenerate.
- **AS6** (automated) `skip_diarization` → `transcript == plain_transcript`, `skipped`, raw + map kept; a later confirm succeeds. `update_speaker_map` re-derives and substitutes whole-word names in the stored summary.
- **AS7** (automated) Embedder error or model missing → plain transcript, state `none`, session reaches Done. Cancel mid-diarization → Cancelled with no speaker data.
- **AS8** (automated) The diarization migration (next free version; v5 if 01's `audio_kept` took v4) applies on a DB at the previous latest version with sessions; old rows read `none`; `user_version` equals the new version.
- **AS9** (automated, vitest) Speakers card: Confirm payload carries typed names; a chip collapses two speakers to one label; Skip emits skip; colours cycle past 5.
- **AS10** (manual real-app) Enable multi-speaker → ~27 MB model downloads with progress → toggle on. Record 60 s of two real voices on the mic, stop → prompt → pick 2 → "Identifying speakers…" → card shows two speakers with quotes → name them → coloured named transcript, summary regenerates using the names.
- **AS11** (manual) Three speakers; collapse a phantom fourth via a chip.
- **AS12** (manual) Mic+System on a video call with two remote participants plus the therapist: three labelled speakers, none duplicated.
- **AS13** (manual) Skip → plain transcript and "Add speakers" bar → assign and confirm later.
- **AS14** (manual, perf) A 50-minute recording diarizes in about 3 min or less on an M-series Mac with the Gemma model present; record time and peak RSS in the PR.
- **AS15** (manual) A signed, notarized build loads the native runtime and diarizes (hardened runtime / dylib loading).

## Design notes
- **Where it runs**: after stop, not in the stream. `stt/stream.rs` is unchanged. Streaming whisper already produced timestamped per-source segments, so there is **no second whisper pass** (an Electron cost we avoid). With the setting on, keep today's `transcript()` output as `plain_transcript` and build `raw_transcript` from the diarized segments.
- **Audio access**: read slices via `Store::audio_samples` (i16 → f32 / 32768). If spec 01 encrypts or deletes chunks, diarization must read through whatever API 01 provides and run before deletion.
- **Modules**: new `diarize/` (`mod.rs` trait + labelling; engine file gated off under `fake-engines`; two-source merge); `text/speakers.rs` (`apply_speaker_map`, `build_raw_transcript_and_map`, name substitution; reuse `strip_brackets` / `js_trim` from `text/` for parity); fake in `engines/`; `store/` (migration + accessors + `MULTI_SPEAKER` setting); `models/manifest.rs` + `models/mod.rs`; `jobs/session.rs` (finalize hook, `num_speakers` through stop); `commands.rs` + registration; `src/lib/api.ts` + ts-rs types (`DiarizationState`, `SpeakerMap`, new `Session` fields); Svelte `SpeakersCard`, `SpeakerBar`, `SpeakerCountPrompt`, hooks in `RecordView`, `SummaryPanel`, `Settings`.
- **Migration, next free version at implementation time** (append-only in `store/migrations.rs`; 01 lands first and takes v4): `ALTER TABLE sessions ADD COLUMN plain_transcript TEXT`, `raw_transcript TEXT`, `speaker_map TEXT`, `diarization_state TEXT NOT NULL DEFAULT 'none'`. No `segments` change; speaker labels live in `raw_transcript`.
- **Events**: reuse the existing session progress event with the "Identifying speakers…" message; the UI reads `diarization_state` from `Session`.
- **Spec author's calls**:
  - Start the summary on plain text and regenerate on confirm — Electron does this and users get a summary even if they never name speakers.
  - Speaker model is optional/lazy — only 27 MB but only therapists doing multi-party sessions need it.
  - Rust reads the raw transcript, the UI doesn't send it — Rust core owns state.
  - Whole-word name substitution — fixes a substring-corruption risk.
  - Count prompt is a modal in the stop flow — mirrors Electron.

## Running the models from Rust: options
Electron needs only the **embedding** model; it must be run on 80-dim Kaldi-style fbank features of each segment slice. All maturity notes below are unverified.

| Option | What | Pros | Cons / maturity to verify |
|---|---|---|---|
| A. Official sherpa-onnx Rust API (if published on crates.io) | Rust wrapper over the C API | Same C++ code as Electron's 1.12.31 → identical features and benchmark | Newest binding, API stability unknown; build.rs may download prebuilt libs (read like npm postinstall); dylib bundling/signing |
| B. `sherpa-rs` (community) | Third-party wrapper including speaker embedding | Many Tauri precedents, ergonomic | Single maintainer, lags upstream, downloads prebuilt binaries in build.rs |
| C. Own thin FFI to the sherpa-onnx C API | ~6 `extern "C"` functions against libs we vendor and hash-pin | Smallest trust surface, no third-party Rust code | We own link scripting and `unsafe`; S–M extra work |
| D. `ort` + in-house fbank | ONNX Runtime bindings; we compute fbank | No sherpa dependency; runtime dylib pinned | `ort` 2.x has been long in release-candidate form (check if stable now); must reimplement fbank exactly (parity test mandatory) or accuracy silently drops |
| E. `candle` / `tract` | Pure Rust | No native lib | CAM++ operator coverage unverified; highest risk |

**Historical spec recommendation, superseded for implementation by the approved execution decision above**: spike C, then A, with D as fallback, because C/A reuse the exact code Electron's benchmarks were measured with. The parity criterion remains unverified, and the notarized-build, size, and symbol checks remain separate verification/release work. The ggml duplicate-symbol risk (whisper-rs-sys vs llama-cpp-sys-2, `DEPENDENCY_REVIEW_ENGINES.md`) does not apply to ONNX Runtime, but verify dylib packaging: Electron already needed rpath fixes for `libonnxruntime`.

## New dependencies (review needed — implementation step 1)
Follow `plans/DEPENDENCY_REVIEW_ENGINES.md`: exact `=` pin, newest stable ≥ 2 weeks old, owners, 90-day downloads, OSV + RustSec, `build.rs` read.
1. One of: official sherpa-onnx crate (+ `-sys`), `sherpa-rs`, or none (own FFI; in-house alternative).
2. Alternative: `ort` + in-house fbank (+ maybe an FFT; try a small in-house radix-2 before adding `rustfft`).
3. Native artifacts: `libonnxruntime` and sherpa libs, bundled into the app and signed. Measure size in the spike; record download source and checksum.
4. Possibly `bindgen` for option C (already transitive via whisper-rs-sys).
5. The model file: hash-pinned artifact (see Models).
No clustering or similarity crate: cosine and greedy matching are ~40 lines.

## Models
- One file: `3dspeaker_speech_campplus_sv_zh_en_16k-common_advanced.onnx`, **28,281,164 bytes (~27 MB)**, Apache-2.0, from the k2-fsa/sherpa-onnx speaker-recognition release assets (Electron plan names the tag; spelling to be verified).
- GitHub release assets are not commit-pinned like the HF URLs in `manifest.rs`. The review must find a pinned mirror or record a GitHub URL with a sha256 computed from the Electron copy at review time. Do not guess a hash.
- Manager changes: `ModelKind::Speaker` (regenerates the ts-rs `ModelKind` type); an `optional` flag on `ModelSpec` so `ensure_all` skips it; an `ensure_one(id)` path and a generic `download_model(id)` command (not a speaker-specific one; spec 03 R17 reuses it for optional LLMs) reusing the existing download/verify/progress machinery; the model-status UI lists it apart from first-run blockers so `FirstRun` isn't gated on it. The engine loads on demand and is dropped after use.

## Out of scope
Live diarization (`plans/DIARIZATION_STREAMING.md`); TinyDiarize; tray submenu UI (spec 08); column encryption (spec 01); Electron data migration; Windows.

## Depends on / interacts with
- **01-encryption-audio-retention**: 01 encrypts the whole DB (SQLCipher), so new columns are covered automatically; no per-column work. Diarization reads audio through `Store::audio_samples` and must finish before `save_transcript` applies the `keep_audio` deletion (01 R13a).
- **03-note-styles-model-choice**: regenerate-on-confirm uses the session's prompt and chosen model.
- **08-app-shell-tray-notifications**: tray "How many speakers?" uses R6's argument; avoid a double "summary complete" notification (initial summary + regenerate).
- **09-session-management-recovery**: a session killed mid-diarization falls back to `none`; copy and regenerate use the labelled transcript when confirmed, plain otherwise.
- **06-diagnostics-telemetry**: log speaker count only, never text.

## Coordination notes (gap-check 2026-11-04)
- **Order with 01:** 01 has landed, so the DB is SQLCipher, the migration number is the next free one, and audio may be deleted at `save_transcript`. Diarization runs in finalize before that point (R13a in 01). Retention must not grow.
- **Model download:** 02 owns the generic `download_model(id)` command, `ModelSpec.optional` and `ensure_one(id)`. Spec 03 reuses them.
- **Native-lib packaging is owned here (PR4/AS15):** add the ONNX Runtime / sherpa dylibs to `bundle.macOS.frameworks` (`tauri.conf.json:28-35`) and extend the dylib/rpath guard in `.github/workflows/release-mac.yml:40-50` to cover them, so a release build fails if one is missing or linked to a non-bundled path.
- **UI mount point:** no session detail view exists until 09. Mount `SpeakersCard`/`SpeakerBar` on the current-session result (`RecordView`/`SummaryPanel`). Verify AS13 ("add speakers later") on the current session; 09 re-mounts both in session detail and re-runs AS13 there.
- **Notices:** the R14 notice is in-app (no native dialog), consistent with 08 R16. The tray speaker submenu is 08 R10, which wires `num_speakers` directly because 02 lands first.
- **Settings:** add a headed "Multi-speaker" section to the existing Settings view; 08 later moves it into its category layout.

## Decisions for Patrick
1. Engine path. **Default: spike own FFI (C), then official crate (A); `ort` + fbank (D) as fallback.**
2. Speaker model optional download (default) vs bundled (+27 MB installer, no first-use wait). **Default: optional download.**
3. Summary timing: start on plain text and regenerate on confirm (Electron parity) vs wait for the user. **Default: Electron parity.**
4. Ship 4+ speakers although Electron's own benchmark says not production-ready. **Default: ship, labelled "4+ (beta)", with the collapse chips.**
5. Include mic+system diarization (implemented in Electron code, "gated out" in its plan). **Default: include.**

## Size
**L**, roughly 8–10 dev days including review and spike. PR split:
1. Dependency review + engine spike + parity measurement (docs and a throwaway bench; no app code). Needs Patrick's approval.
2. Model manager optional spec + download command + setting + Settings toggle (S).
3. `text/speakers.rs`, `diarize/` with fake embedder, diarization migration (next free version), store, commands, finalize hook — AS1–AS8 (M).
4. Real engine wiring, bundling, signing — AS14, AS15 (M).
5. Svelte UI — AS9–AS13 (M).
