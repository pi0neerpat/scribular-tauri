# Remote LLM for uitest (`remote-llm`)

## What it does
- `remote-llm` is an opt-in Cargo feature (`remote-llm = []`, `src-tauri/Cargo.toml`). A build with it sends summary generation to a host `llama-server`, so uitest runs in the VM don't wait on the VM's CPU.
- `RemoteSummarizer` (`src-tauri/src/llm/remote.rs`) talks to llama-server's `/tokenize` and streaming `/completion`.
- `summarizer_loader` (`src-tauri/src/lib.rs`) picks `RemoteLoader` when the feature is compiled in and `SCRIBULAR_LLM_URL` is non-empty. Otherwise it uses the on-device `LlamaLoader`.
- The module header says it is never compiled into shipping builds: shipping builds don't enable the feature.

## How it works
- **Selection** (`src-tauri/src/lib.rs`, `remote_url` in `remote.rs`): an empty value counts as unset. When selected, the app logs `SCRIBULAR_LLM_URL set: summaries use <url>` at INFO (`llm` target).
- **Loader** (`RemoteLoader`): `model_id()` and `ready()` delegate to `LlamaLoader`, so onboarding and session records are unchanged and the local model must still be downloaded and verified. `load` returns `NotReady` if it is not.
- **URL validation** (`RemoteSummarizer::new`): the URL must parse and be `http` or `https`; anything else (e.g. `localhost:5801`, which parses as scheme `localhost`) is an `LlmError::Engine`. A trailing `/` is added to the path so `join` keeps it.
- **Timeouts**: 5 s connect and 300 s per read, so a wrong host or stalled server fails the job instead of hanging it. The read timeout is long enough for a request queued behind busy server slots.
- **Tokenizing**: `count_tokens` calls `/tokenize` with `add_special` and `parse_special` false. `prompt_tokens` formats the prompt with `format_gemma4_user_prompt` and sends both flags true, mirroring the local engine's `tokenize(add_bos, parse_special)` for BOS parity. `context_tokens` comes from `llama::params` (8192).
- **Generation**: one `/completion` request per `Generation` (`stream: true`, `cache_prompt: true`, `n_predict` = `max_tokens`, sampling values from `llama::params`). The prompt is built locally with `format_gemma4_user_prompt`, not the server's template. Outputs return in request order.
- **SSE parsing** (`stream` in `remote.rs`): bytes go into a buffer and only whole `\n\n`-terminated events are decoded, so a UTF-8 character split across HTTP chunks stays intact. A stream that ends without `stop` is an error.
- **Cancel**: `generate` drains the streams from the worker thread every 50 ms, checks the cancel flag, returns `LlmError::Cancelled`, and aborts all stream tasks.
- **Errors**: any transport, non-2xx, or bad-JSON failure becomes `LlmError::Engine("<url>: <reason>")`.
- **uitest forwarding** (`tools/uitest/Sources/uitest/AppLifecycle.swift`): `launch` passes `SCRIBULAR_LLM_URL` to the app when it is set and non-empty in uitest's own environment.

## Config and data
- Env var: `SCRIBULAR_LLM_URL`, e.g. `http://192.168.64.1:5801`.
- Host server (from the "Host llama-server (VM)" section of `tools/uitest/README.md`): `llama-server -m gemma-4-E2B-it-GGUF-Q4_K_M.gguf --host 192.168.64.1 --port 5801 -np 4 -c 32768`
  - `--host 192.168.64.1`: the Softnet bridge address. Loopback is unreachable from the VM, and Softnet already allows `192.168.64.1/32`. The address exists only while a VM is running, so start the VM first.
  - `-c 32768`: 8192 x 4. llama-server splits `-c` across `-np` slots, so each slot keeps the 8192-token context the on-device summarizer uses.
  - `-np 4`: four slots, which `-c` is divided by. The README does not give a separate reason for the count of four.
  - Check from the VM: `curl http://192.168.64.1:5801/health`.
- Build: `APPLE_SIGNING_IDENTITY="Scribular Local Dev" TAURI_SIGNING_PRIVATE_KEY="<throwaway key>" npm run tauri -- build --bundles app --features remote-llm` (also set `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` if the key has one).
- Run: `SCRIBULAR_LLM_URL=http://192.168.64.1:5801 swift run --package-path tools/uitest uitest <scenario> --app <path>`.
- No data is stored by this feature: no settings, files, or session fields were added.

## How to test it
- Rust, with the feature: `cargo test --manifest-path src-tauri/Cargo.toml --features remote-llm llm::remote`. The tests in `remote.rs` use a stub HTTP server on loopback (tokenize, streaming, chunk-split UTF-8, batch order, cancel, unreachable server, request body, `remote_url`).
- Rust, without it: `cargo test --manifest-path src-tauri/Cargo.toml`. The `remote` module is not compiled.
- Swift: `swift test --package-path tools/uitest`.
- Manual: start the host server, run `curl http://192.168.64.1:5801/health` in the VM, launch the `remote-llm` build with the env var set, and look for the `SCRIBULAR_LLM_URL set: summaries use <url>` log line and `/completion` requests in the llama-server log.

## Known limits
- A summary over the remote path was not verified end to end in the real app. No uitest path triggers a summary: sample sessions cannot be Regenerated, and the `system-audio-smoke` scenario is stale because it clicks an `App audio` control that Settings no longer shows.
- llama-server has no authentication and receives transcript text over plain HTTP. Bind the bridge address, not `0.0.0.0`, or it is open to the whole LAN (`tools/uitest/README.md`).
- The host must serve the same GGUF as the app manifest (`gemma-4-E2B-it-GGUF-Q4_K_M.gguf`), or results won't match on-device.
- Only generation moves to the host. The on-device model is still required for readiness (onboarding downloads and verifies it).
