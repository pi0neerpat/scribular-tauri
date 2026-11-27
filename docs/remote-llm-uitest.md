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
- **uitest forwarding** (`tools/uitest/Sources/uitest/AppLifecycle.swift`): `launch` passes the resolved URL (below) to the app as `SCRIBULAR_LLM_URL`.
- **Automatic host server in the VM** (`tools/uitest/Sources/UITestCore/HostLLM.swift`, PR #65): `HostLLM.resolve` picks where summaries run.
  - A `SCRIBULAR_LLM_URL` in uitest's environment wins and nothing is probed. Empty forces on-device.
  - In a VM (`sysctl kern.hv_vmm_present` is 1) uitest checks `http://192.168.64.1:5801/health` (2 s timeout; HTTP 200 with JSON `status` exactly `"ok"`). Healthy: it passes that URL to the app and prints `info host llama-server: <url>`. Down: it prints a `WARN` and runs on-device.
  - On the host it stays on-device and probes nothing.
  - `tools/uitest/tauri-debug-overlay.json` sets `build.features: ["remote-llm"]`, so debug builds made with the overlay include the feature.
  - **Log check** (`main.swift`): when uitest passed a URL, the lines this run added to `<runDir>/app.log` must contain `SCRIBULAR_LLM_URL set: summaries use <same url>`. Otherwise the run fails with `app ignored SCRIBULAR_LLM_URL: build lacks --features remote-llm`; a missing `app.log` also fails.
  - `uitest doctor` prints an `info host llama-server:` line (ok, unreachable, or not probed with the override, credentials and query values hidden). It never fails doctor. The app logs a non-empty URL verbatim at launch, so do not include credentials or sensitive query values in `SCRIBULAR_LLM_URL`.

## Config and data
- Env var: `SCRIBULAR_LLM_URL`, e.g. `http://192.168.64.1:5801`.
- Host server (from the "Host llama-server (VM)" section of `tools/uitest/README.md`): `llama-server -m gemma-4-E2B-it-GGUF-Q4_K_M.gguf --host 192.168.64.1 --port 5801 -np 4 -c 32768`
  - `--host 192.168.64.1`: the Softnet bridge address. Loopback is unreachable from the VM, and Softnet already allows `192.168.64.1/32`. The address exists only while a VM is running, so start the VM first.
  - `-c 32768`: 8192 x 4. llama-server splits `-c` across `-np` slots, so each slot keeps the 8192-token context the on-device summarizer uses.
  - `-np 4`: four slots, which `-c` is divided by. The README does not give a separate reason for the count of four.
  - Check from the VM: `curl http://192.168.64.1:5801/health`.
- Build: `APPLE_SIGNING_IDENTITY="Scribular Local Dev" TAURI_SIGNING_PRIVATE_KEY="<throwaway key>" npm run tauri -- build --bundles app --features remote-llm` (also set `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` if the key has one).
- Run: in the VM no variable is needed. Elsewhere, or for another server: `SCRIBULAR_LLM_URL=http://192.168.64.1:5801 swift run --package-path tools/uitest uitest <scenario> --app <path>`.
- No data is stored by this feature: no settings, files, or session fields were added.

## How to test it
- Rust, with the feature: `cargo test --manifest-path src-tauri/Cargo.toml --features remote-llm llm::remote`. The tests in `remote.rs` use a stub HTTP server on loopback (tokenize, streaming, chunk-split UTF-8, batch order, cancel, unreachable server, request body, `remote_url`).
- Rust, without it: `cargo test --manifest-path src-tauri/Cargo.toml`. The `remote` module is not compiled.
- Swift: `swift test --package-path tools/uitest` (114 tests at merge; `HostLLMTests` cover resolution, health parsing, the log check and redaction).
- End to end (VM, host server running): the `host-llm-smoke.uitest` scenario passed 20/20 steps (call audio, Summary via the host server, resolved URL in `app.log`) at head eb3e6da. It lived in the task state directory and was not committed.
- Manual: start the host server, run `curl http://192.168.64.1:5801/health` in the VM, launch the `remote-llm` build with the env var set, and look for the `SCRIBULAR_LLM_URL set: summaries use <url>` log line and `/completion` requests in the llama-server log.

## Known limits
- The host address `192.168.64.1:5801` is fixed in code (`HostLLM.address`); a second VM network needs a code change.
- A host server that is down is only a warning: the run falls back to the VM CPU and summaries are slow (use `expect "Summary·" timeout 400s`).
- The `host-llm-smoke` scenario is not in the repo, and `system-audio-smoke` may still be stale (it clicks an `App audio` control that Settings no longer shows).
- llama-server has no authentication and receives transcript text over plain HTTP. Bind the bridge address, not `0.0.0.0`, or it is open to the whole LAN (`tools/uitest/README.md`).
- The host must serve the same GGUF as the app manifest (`gemma-4-E2B-it-GGUF-Q4_K_M.gguf`), or results won't match on-device.
- Only generation moves to the host. The on-device model is still required for readiness (onboarding downloads and verifies it).
