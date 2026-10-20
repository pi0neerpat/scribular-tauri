# Plan: Tauri MVP of Scribular's core flow

**Status:** Approved scope, dependencies pending approval
**Goal:** A working Tauri 2 app that runs the whole core loop end to end, entirely
on-device: **record (live transcript) → label speakers → summarize with a template →
save encrypted → review/edit/regenerate**. It exists to test the architecture changes
proposed for a rewrite, not to replace the Electron app yet.
**Location:** This repo (`scribular-tauri/`). The Electron app at
`../notes-assistant-electron/` is used only as reference and isn't changed.

## Decisions taken

| Question | Decision |
|---|---|
| Repo location | `../scribular-tauri/` (sibling of the Electron app) |
| Keep audio after processing? | Setting `keep_audio`, **default off**: audio is deleted once the transcript is saved. When on, audio is kept encrypted in the database |
| Streaming transcription in MVP? | **Yes.** Transcribe while recording and show a live transcript (milestone M5b) |
**Platform:** macOS (Apple Silicon) first. Windows is a later milestone (M7).

---

## 1. What "core flow" means (scope)

Taken from the current app (`docs/UX.md`, `src/service/*`, `src/store/*`):

| # | Core feature | Electron source today | In MVP |
|---|---|---|---|
| 1 | Record mic | `renderer/features/audio.js` (Web Audio) | ✅ |
| 2 | Record system/app audio (telehealth) | `AudioCapCLI` spawned from `audio/audio.js` | ✅ |
| 3 | Pick input mode (mic / system / both) + target app | `store/defaults.js` `audioInputType`, `audioSystemAppId` | ✅ |
| 4 | Transcribe with whisper `ggml-base.bin` | `audioWorker.js` spawns whisper CLI | ✅ in-process |
| 5 | `Client:` / `You:` labels by merging timestamps (two sources) | `mergeDiarizedSegments` in `audioWorker.js:350` | ✅ |
| 6 | Clean the transcript (noise tokens, repeats) | regex chain in `summarize.js` `processTranscript` | ✅ |
| 7 | Chunked summarization (3500-char chunks, then a combine pass) | `summarize.js` | ✅ in-process |
| 8 | Prompt templates (DORA, Narrative, EFT, …), choose per session | `store/defaults.js` `prompts` | ✅ (seed defaults, read-only + pick) |
| 9 | Step-by-step progress UI | `session:progress` events | ✅ |
| 10 | Cancel transcription / summarization | `AbortController` (summary only; transcription *can't* be cancelled today, see the TODO at `audioWorker.js:98`) | ✅ both |
| 11 | Session list + detail, edit transcript/summary, regenerate | `views/session.js`, `features/sidebar.js` | ✅ |
| 12 | Sessions encrypted at rest | `store/sessions.js` + `safeStorage` | ✅ SQLCipher |
| 13 | Model download with progress on first run | Ollama `pull()` | ✅ (one model) |
| 14 | Keep or auto-delete session audio | `saveAudio.js`, `audioEncryption.js` | ✅ toggle, default delete |
| 15 | Live transcript while recording | — (new; today transcription starts only at stop) | ✅ |

**Out of scope for MVP:** license/trial, onboarding, auto-update, analytics/error
reporting, multi-speaker embedding diarization (sherpa-onnx), model catalog UI,
editing prompt templates, tray menu, audio playback/export, re-transcribing from
kept audio, sample sessions, custom window chrome, Windows.

---

## 2. Architecture

```
┌──────────────── Svelte 5 + TS (webview) ────────────────┐
│  RecordView   SessionList   SessionDetail   Settings    │
│        ▲ typed commands / events (ts-rs types)          │
└────────┼────────────────────────────────────────────────┘
         │
┌────────▼──────────────── Rust core ─────────────────────┐
│  commands.rs   ← the only IPC surface, generated TS types│
│  jobs/         ← SessionJob state machine (owns truth)   │
│  audio/        ← mic (cpal) + system (sidecar) → PCM     │
│  stt/          ← whisper-rs, streaming windows, abort    │
│  llm/          ← llama-cpp-2 (Metal), cancel token       │
│  text/         ← cleanup, merge, chunking (pure fns)     │
│  store/        ← rusqlite + SQLCipher, key in Keychain   │
│  models/       ← GGUF download + sha256 check            │
└──────────────────────────────────────────────────────────┘
```

### Decisions

| Decision | Choice | Why |
|---|---|---|
| Shell | **Tauri 2** | Small bundle; Rust lets inference run in-process |
| STT | **`whisper-rs`** (Metal), reuse `ggml-base.bin` | No spawned process; the abort callback makes transcription cancellable |
| LLM | **`llama-cpp-2`** (Metal) + GGUF `gemma-2-2b-it Q4_K_M` | Removes Ollama, the port 11434 problem, PATH confusion and orphan processes (`FIX_WINDOWS_OLLAMA_PORT_ORPHAN.md`, `FIX_MODEL_PROVENANCE_AND_SCOPED_STOP.md`) |
| Mic capture | **`cpal`** in Rust | Audio never passes through the webview, so there are no IPC chunk messages |
| System audio (MVP) | **Reuse `AudioCapCLI` as a Tauri sidecar** | Known to work. Native Core Audio process taps are M6 so they don't block the demo |
| Resample to 16 kHz mono | Port the existing downsampler in-house | Small amount of code, and the dependency policy prefers in-house |
| State | **Rust `SessionJob` state machine** emits events; UI only renders them | Fixes the root cause in `FIX_TRANSCRIPT_CLOBBER_AND_STOP.md` |
| Storage | **SQLite + SQLCipher** (`rusqlite` `bundled-sqlcipher`), key in macOS Keychain (`keyring`) | No 100-session cap; full-text search possible later; real migrations |
| Audio at rest | **PCM chunks stored as BLOBs in the SQLCipher database**, never as WAV files | Encrypted from the first byte with no extra crypto dependency. Chunks are written during recording, so a crash keeps the audio. `whisper-rs` reads samples straight from memory, so no WAV file is needed |
| Transcription timing | **Streaming**: whisper runs on fixed windows while recording | The transcript is nearly finished when the user presses Stop; only the last window remains |
| Frontend | **Svelte 5 + TypeScript + Vite** | Replaces 1.8k-line hand-written DOM views with components |
| IPC types | **`ts-rs`** generates TS types from the Rust IPC structs/enums; thin hand-written `invoke`/`listen` wrappers in `src/lib/api.ts`; a Rust test checks every registered command appears in `api.ts` | A changed type or renamed command fails in tests/type-check instead of silently at runtime. `tauri-specta` was rejected: pre-release only, single maintainer (see `DEPENDENCY_REVIEW_M0.md`) |
| Engines behind traits | `trait Transcriber`, `trait Summarizer`, `trait SystemAudio` | Fake implementations replace the `--e2e-test` flag so tests are fast and deterministic |

### Session job state machine

```
Idle → Recording{live segments} → Finalizing{last window} → Summarizing{chunk i/n | combining} → Done
                                        │                          │
                                        └──────── Cancelled ◄──────┘     any state → Failed{reason}
```

Audio cleanup runs on `Done`, and also on `Cancelled`/`Failed` once the transcript
is saved: if `keep_audio` is off, the `audio_chunks` rows are deleted.

- Every transition is saved to `sessions.status` **before** an event is emitted, so a
  crash leaves a resumable row.
- Event `session://progress` `{ sessionId, state, stepIndex, stepCount, message }`.
  It is the same shape as today's `session:progress` so the UI design carries over.
- One `CancellationToken` per job. Whisper checks it through `abort_callback`. The
  llama sampling loop checks it on every token.
- **Stop** during Summarizing → put back the previous summary (same decision as the
  existing plan).
- Regenerate = a new job that starts at `Summarizing` with a different `promptId`.

### Data model (SQLCipher)

```sql
sessions(id TEXT PK, title TEXT, created_at INTEGER, input_mode TEXT,
         status TEXT, error TEXT, prompt_id TEXT, model_id TEXT,
         transcript TEXT, summary TEXT, duration_ms INTEGER,
         audio_kept INTEGER)                       -- 1 if audio rows were kept
segments(session_id TEXT, source TEXT,             -- 'mic' | 'system'
         start_ms INTEGER, end_ms INTEGER, text TEXT)  -- live transcript, written per window
audio_chunks(session_id TEXT, source TEXT, seq INTEGER,
             pcm BLOB)                             -- 16 kHz mono i16, ~5 s per row
prompts(id TEXT PK, name TEXT, description TEXT, text TEXT, is_default INTEGER, starred INTEGER)
settings(key TEXT PK, value TEXT)                  -- includes keep_audio = 'false'
```

The whole database is encrypted, so no per-field encryption is needed.
`PRAGMA secure_delete = ON` overwrites deleted audio pages instead of only marking
them free. An hour of audio from one source is about 115 MB as i16; the settings
screen should say so next to the `keep_audio` toggle. The 7 default
prompts are copied word for word from `src/store/defaults.js` in a seed migration.

---

## 3. Milestones

Each milestone ends with something you can demo and a check that confirms it.

### M0 — Scaffold (½ day)
- Scaffold by hand instead of running `create-tauri-app`, which would pull unreviewed,
  unpinned versions. Write `package.json` and `Cargo.toml` with the exact pins from
  `DEPENDENCY_REVIEW_M0.md`.
- `ts-rs` set up: `#[derive(TS)] #[ts(export, export_to = "../src/lib/types/")]` on IPC
  types; generated files are committed.
- `src/lib/api.ts` with one example command (`app_version`) and one example event, plus
  the Rust command-list test.
- CI: `cargo test` → `git diff --exit-code src/lib/types` (generated types are up to
  date) → `svelte-check` → `vitest`.
- **Done when:** `cargo tauri dev` opens a window that shows the result of
  `app_version`. Renaming a field on the Rust side fails `svelte-check`, and removing the
  wrapper from `api.ts` fails `cargo test`.

### M1 — Storage + sessions UI (1 day)
- `store/` with SQLCipher, Keychain key, migrations, prompt seed.
- Commands: `list_sessions`, `get_session`, `update_session`, `delete_session`, `list_prompts`.
- Svelte: sidebar list, session detail with editable transcript/summary (save on blur).
- **Done when:** sessions survive a restart. Running `sqlite3` on the DB file without
  the key can't read it.

### M2 — Text pipeline, pure functions (½ day)
- Port to `text/` with unit tests that use fixtures taken from the current JS:
  - transcript cleanup regex chain (`summarize.js` `processTranscript`)
  - `mergeDiarizedSegments` → `Client:`/`You:` output
  - `chunkTranscript` (3500 chars, rejects < 10 words)
- **Done when:** the Rust output matches the JS output on the same fixtures
  (the fixtures are generated once by a small node script in this repo).

### M3 — Summarize in-process (1–1.5 days)
- `models/`: download the GGUF to `appDataDir/models` with progress events and sha256
  check, and resume support.
- `llm/`: load once and keep loaded. Build the prompt the same way
  (`{prompt}\n<transcript>\n{chunk}\n</transcript>`), run each chunk, then the combine
  pass. Stream tokens to the UI.
- `jobs/`: `Summarizing → Done | Cancelled | Failed` with step counts.
- Command `regenerate_summary(sessionId, promptId)`, `cancel_job(sessionId)`.
- **Done when:** pasting a sample transcript and pressing Regenerate streams a summary.
  Stop halts output in under 1 s and puts back the previous summary.

### M4 — Transcribe in-process (1 day)
- `stt/`: whisper-rs with `ggml-base.bin` (bundled as a Tauri resource), segment
  timestamps, `suppress_nst`, abort callback tied to the job token.
- Command `transcribe_file(path)` for development.
- **Done when:** a fixture WAV transcribes, and cancelling partway returns within 1 s.
  WAV is only for test fixtures; the app itself passes PCM samples from memory.

### M5a — Record and full flow, batch (1.5–2 days)
- `audio/mic.rs`: cpal input → ring buffer → 16 kHz mono → appended to `audio_chunks`
  **while recording** (so a crash doesn't lose the audio).
- `audio/system.rs`: `AudioCapCLI` sidecar (`tauri-plugin-shell` sidecar) with
  `--app` target. Read the PCM it outputs into the same chunk writer with
  `source = 'system'`.
- Commands: `list_audio_apps`, `start_recording(mode, appId)`, `stop_recording`.
- Stopping starts the job: transcribe each source from its chunks → merge → clean →
  Summarizing → Done → audio cleanup according to `keep_audio`. The UI navigates to
  the session and shows progress.
- Settings view: input mode, target app, default prompt, **`keep_audio` toggle (off by
  default)** with the storage-size note.
- Mic and screen-recording permissions: add `Info.plist` usage strings, and a button
  that opens System Settings when access is denied.
- **Done when:** the full loop works in batch mode. With `keep_audio` off, the session's
  `audio_chunks` row count is 0 after Done. With it on, the rows remain and the DB file
  still can't be read without the key.

M5a ships batch mode first so the whole loop is proven before streaming changes when
transcription runs.

### M5b — Streaming transcription (2 days)
- `stt/stream.rs`: one worker per source. Every ~15 s of new audio, transcribe a
  ~20 s window (5 s overlap with the previous one). Use segment timestamps to drop
  repeated words in the overlap: keep only segments whose `start_ms` is after the
  last saved `end_ms`.
- Before each window, check the RMS energy and skip silent windows. Silence is where
  whisper invents phrases like "Thank you for watching".
- Save each window's segments to `segments` and emit `session://segments`. The record
  view shows a live `Client:`/`You:` transcript using the same merge function from M2,
  run on what's available so far.
- If the worker falls behind real time (more than 2 windows queued), show a "catching
  up" indicator. Never drop audio; the queue is always drained at stop.
- On Stop: `Finalizing` transcribes only the remaining tail, then continues to
  Summarizing as before.
- Load the whisper model once when recording starts. Unload it before summarizing if
  the machine has 8 GB of RAM or less.
- **Done when (MVP demo):** during a 5-minute FaceTime call in "both" mode, the
  transcript appears within about 20 s of speech. The summary starts within 10 s of
  pressing Stop. The final session is labelled `Client:`/`You:` with a DORA summary.
  No network traffic after the model download (check with Little Snitch or by
  turning off Wi-Fi). On the same audio, the transcript has about the same word
  count as the batch output from M5a (within 5%).

### M6 — Native system audio (stretch, 2 days)
- Replace the sidecar with a Core Audio process tap (macOS 14.4+) through a small
  Swift static library (`swift-rs`) or `objc2-core-audio`.
- **Done when:** `AudioCapCLI` is deleted and M5b still passes.

### M7 — Windows (stretch, 2–3 days)
- WASAPI process loopback (through `windows` crate), CUDA/Vulkan or CPU build flags for
  whisper/llama, and DPAPI or Credential Manager for the DB key through `keyring`.

**MVP = M0–M5b, about 8–9 working days.**

---

## 4. Testing

| Layer | Tool | What it covers |
|---|---|---|
| Rust unit | `cargo test` | `text/` equivalence with the JS, state machine transitions, cancel semantics, migrations, overlap de-duplication in streaming windows, audio cleanup for each `keep_audio` value and end state |
| Rust integration | `cargo test --features fake-engines` | Full job using `FakeTranscriber`/`FakeSummarizer`; checks event order and saved state |
| Frontend | `vitest` + `@testing-library/svelte` | Components render progress, error and cancelled states from events |
| E2E | Manual demo script (M5b check) | `tauri-driver` doesn't support macOS WebDriver, so there is no automated end-to-end test on Mac |
| Model smoke | `cargo test --ignored` | Real whisper and llama on a 30 s fixture; run before a demo, not in CI |

The fake-engine traits replace `--e2e-test`: tests replace the engines through the
interface instead of the app checking a test flag inside its own code.

---

## 5. New dependencies (need approval per the dependency policy)

This is a new project, but the policy still applies. Each package below needs the
4-point review (maintainers/downloads/last publish, audit + `postinstall`/`build.rs`
review, exact pin, in-house check) **before** it is added. Versions are pinned exactly
at approval time.

| Package | Purpose | In-house alternative? |
|---|---|---|
| `tauri`, `tauri-build`, `@tauri-apps/api`, `@tauri-apps/cli` | Shell | No |
| `tauri-plugin-shell` | `AudioCapCLI` sidecar (M5a/M5b only, removed in M6) | No |
| `ts-rs`, `ts-rs-macros` | Generated IPC types (✅ reviewed in M0 review) | Partly: the thin command wrappers stay in-house |
| `whisper-rs` | STT bindings (`build.rs` compiles whisper.cpp) | No |
| `llama-cpp-2` | LLM bindings (`build.rs` compiles llama.cpp) | No |
| `cpal` | Mic capture | No |
| `rusqlite` (`bundled-sqlcipher`) | Encrypted DB | No |
| `keyring` | Keychain/Credential Manager | Possible on macOS only; keep for cross-platform |
| `reqwest`, `sha2` | Model download + verify | No |
| `tokio`, `tokio-util` | Async, `CancellationToken` | No |
| `serde`, `serde_json`, `thiserror` | Standard | No |
| `svelte`, `@sveltejs/vite-plugin-svelte`, `vite`, `typescript` (6.x, because `svelte-check` doesn't support 7), `svelte-check`, `vitest` | Frontend (✅ reviewed in M0 review) | No |
| `@testing-library/svelte` | Component tests (M1) | No |

**Note:** `whisper-rs` and `llama-cpp-2` compile C++ in `build.rs`. Review those build
scripts the same way as an npm `postinstall`.

---

## 6. Risks

| Risk | Mitigation |
|---|---|
| whisper.cpp and llama.cpp both link ggml, causing duplicate symbols at link time | Check in M0 by linking both crates in a hello-world. Fallback: run STT in a `whisper-rs` dylib, or use one shared ggml through a crate feature |
| Metal build flags or Xcode toolchain problems | Pin the Xcode version in CI; CPU build as fallback |
| GGUF output quality differs from Ollama's `gemma2:2b` (chat template, sampling) | Use the model's own chat template from GGUF metadata; run each default prompt on the same fixture in both apps and compare by eye |
| Memory: whisper and LLM loaded together on 8 GB Macs | Unload whisper before the summarization step; load LLM lazily |
| Screen-recording permission behaviour with the sidecar | Already handled the same way in the Electron app |
| Streaming whisper can't keep up on older Macs (base model, two sources) | Process both sources on one worker in turn; the "catching up" indicator; batch fallback from M5a stays available |
| Words split or repeated at window edges | 5 s overlap + timestamp de-duplication; the M5b check compares against batch output |
| Writing audio rows to the DB every few seconds slows the UI | WAL mode; the audio writer gets its own connection on its own thread |

---

## 7. Next step

The M0 dependency review is done (`plans/DEPENDENCY_REVIEW_M0.md`) and awaits
approval, along with installing the Rust toolchain (`rustup`, stable). M0 starts after
that. The remaining packages in section 5 are reviewed before the milestone that needs
them: M1 (`rusqlite`, `keyring`, `@testing-library/svelte`), M3 (`llama-cpp-2`,
`reqwest`, `sha2`, `tokio`, `tokio-util`, `thiserror`), M4 (`whisper-rs`), M5a (`cpal`,
`tauri-plugin-shell`).
