# Remote LLM for uitest on the VM — design (short)

Date: 2026-11-18 · Status: approved

## Why

Verifying the note style coach with uitest inside the Tart VM is too slow because
summaries run on the VM's CPU-bound local Gemma. `notes-assistant-electron` solved
this by pointing at Ollama on the host. Scribular should be able to send summary
generation to a `llama-server` on the host, for testing only.

## Goals / non-goals / anti-goals

- Goal: a uitest run on the VM produces summaries via the host `llama-server`
  with the same model, prompt format, context budget and sampling as on-device.
- Non-goal: a user-facing setting, Ollama support, remote transcription.
- Anti-goal: any shipping build able to send transcripts off the device.

## Approaches considered

1. **Picked:** Cargo feature `remote-llm` (compiled into test builds only) + env var
   `SCRIBULAR_LLM_URL` read at launch; uitest forwards the env var to the app.
2. Env var honoured in all release builds: rejected, hidden off-device switch in
   the shipped app.
3. Ollama on the host: rejected, no tokenize endpoint, so the VM would still load
   the GGUF vocab and the model tag could drift from the bundled GGUF.

## Design

- `src-tauri/src/llm/remote.rs` (behind `#[cfg(feature = "remote-llm")]`):
  `RemoteLoader` / `RemoteSummarizer` implementing the existing traits
  (`llm/mod.rs:55-90`), using the existing `reqwest` dependency.
  - `context_tokens` = 8192 (same as `LlamaParams` default, `llama.rs:58`).
  - `count_tokens` → `POST /tokenize {content}`; `prompt_tokens` → same, on
    `format_gemma4_user_prompt(prompt)` (reuse it from `llama.rs`; move it to
    `prompt.rs` or make it shared so it exists in all builds).
  - `generate` → one streaming `POST /completion` per request, run concurrently
    (server has `-np 4`), with the formatted prompt, `n_predict = max_tokens`,
    temperature/top_p from `LlamaParams`. Emits `GenEvent::Token`/`Finished`;
    checks `cancel` per streamed chunk and returns `Cancelled`.
  - Never use the server's `/apply-template`: Gemma 4's canonical template adds a
    `<|think|>` turn the app deliberately omits.
  - `model_id()` = the selected model id, unchanged, so sessions record the same id.
- `lib.rs:60` `summarizer_loader`: when built with `remote-llm` and
  `SCRIBULAR_LLM_URL` is set and non-empty, return `RemoteLoader`; else `LlamaLoader`.
  Log one INFO line naming the URL at startup.
- uitest `AppLifecycle.swift:54`: copy `SCRIBULAR_LLM_URL` from uitest's own
  environment into `OpenConfiguration.environment` when set.
- Model readiness/onboarding unchanged: the VM still uses the cached GGUF from
  `~/Library/Caches/scribular-uitest/models`.
- Docs: a "Host llama-server" section in `tools/uitest/README.md`:
  `llama-server -m gemma-4-E2B-it-GGUF-Q4_K_M.gguf --host 0.0.0.0 --port 5801 -np 4`,
  VM URL `http://192.168.64.1:5801` (Softnet already allows `192.168.64.1/32`),
  build with `npm run tauri -- build --bundles app -- --features remote-llm`.

## Verified 2026-11-18 from the VM

`http://192.168.64.1:5801`: `/health` ok; serves the same GGUF file name; `/tokenize`
parses `<|turn>`/`<turn|>` as single tokens (105/106); 4 slots; 44 tok/s.

## Tests

- Unit test of `RemoteSummarizer` against a tiny in-process stub HTTP server
  (tokenize + streamed completion + cancel).
- One real uitest run on the VM with `SCRIBULAR_LLM_URL` set (e.g. AS11).

## Left to the builder

Exact streaming parse, error mapping (unreachable server → `LlmError::Engine`
with the URL), the `--features` passthrough syntax for `tauri build`.
