# Remote LLM for uitest Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A `remote-llm` test build of Scribular sends summary generation to a host `llama-server` when `SCRIBULAR_LLM_URL` is set, so uitest runs on the Tart VM are fast.

**Architecture:** A new `RemoteSummarizer`/`RemoteLoader` implements the existing `Summarizer`/`SummarizerLoader` traits over llama-server's HTTP API (`/tokenize`, streaming `/completion`). `lib.rs` picks it only when compiled with `--features remote-llm` and the env var is set. uitest forwards the env var to the launched app.

**Tech Stack:** Rust (existing `reqwest =0.13.5` async client + `serde_json`, driven via `tauri::async_runtime`), Swift (uitest `AppLifecycle`).

**Spec:** `plans/2026-11-18-remote-llm-uitest-design.md`

## Global Constraints

- No new crates. Use `reqwest` (already `default-features = false, features = ["native-tls"]`; `Response::chunk()` needs no extra feature) and `tauri::async_runtime::block_on`, as `license/api.rs:67` does.
- Shipping builds must not contain the remote path: everything new in Rust is behind `#[cfg(feature = "remote-llm")]`.
- Same behaviour as on-device: prompt via `format_gemma4_user_prompt` (`llm/llama.rs:79`), never the server's `/apply-template` (it adds a `<|think|>` turn); `context_tokens = params(id).context_tokens` (8192); sampling `temperature`, `top_p`, `top_k`, `repeat_last_n`, `repeat_penalty` from `params(id)` (`llama.rs:57-64`).
- `model_id()` and `ready()` are unchanged from `LlamaLoader` (delegate to it).
- Env var name: `SCRIBULAR_LLM_URL` (e.g. `http://192.168.64.1:5801`). Empty string = unset.
- Branch: create `feat/remote-llm-uitest` from `main`. Do not stage the pre-existing uncommitted `src-tauri/src/commands.rs` change; ask the user about it if it blocks the checkout.

## Review Focus

1. Host server unreachable / wrong URL → summary job fails with `LlmError::Engine` naming the URL, not a hang: the client sets a 5 s `connect_timeout` and a 300 s per-read `read_timeout` (a stalled server between SSE chunks also fails). Owned by Task 1 (`unreachable_server_is_engine_error`).
2. Cancel mid-stream → `generate` returns `Cancelled` promptly and stops reading. Task 1 (`cancel_stops_generation`).
3. A streamed UTF-8 token or SSE event split across HTTP chunks → text is reassembled, not dropped or garbled. Task 1 (`events_split_across_chunks`).
4. Batch of several requests → outputs returned in request order regardless of finish order. Task 1 (`batch_returns_in_request_order`).
5. Feature compiled in but env var unset → app uses on-device `LlamaLoader` exactly as today. Task 2 (`remote_url_reads_env`).

---

### Task 1: `RemoteSummarizer` over llama-server HTTP

**Files:**
- Modify: `src-tauri/Cargo.toml` (`[features]`: add `remote-llm = []` with a one-line comment)
- Modify: `src-tauri/src/llm/mod.rs` (add `#[cfg(all(feature = "remote-llm", not(feature = "fake-engines")))] pub mod remote;`)
- Modify: `src-tauri/src/llm/llama.rs:79` (make `format_gemma4_user_prompt` `pub(crate)`)
- Create: `src-tauri/src/llm/remote.rs` (impl + `#[cfg(test)] mod tests` with a stub server)

**Interfaces:**
- Produces: `pub struct RemoteSummarizer { base: reqwest::Url, client: reqwest::Client, params: llama::Params }`
- Produces: `impl RemoteSummarizer { pub fn new(base: &str, params: llama::Params) -> Result<Self> }` (invalid URL → `LlmError::Engine`)
- Produces: `impl Summarizer for RemoteSummarizer`

HTTP contract (verified against the host server 2026-11-18):
- `POST {base}/tokenize` body `{"content": text}` → `{"tokens":[...]}`; count = `tokens.len()`. Special tokens like `<|turn>` parse as one token by default.
- `POST {base}/completion` body `{"prompt", "n_predict": max_tokens, "temperature", "top_p", "top_k", "repeat_last_n", "repeat_penalty", "stream": true, "cache_prompt": true}` → SSE lines `data: {"content":"…","stop":false}` … final event has `"stop":true`.

- [ ] **Step 1: Write the stub server and failing tests** in `remote.rs` tests

Stub: `std::net::TcpListener` on `127.0.0.1:0`, a thread that, per connection, reads the request, routes on path, and writes a canned `HTTP/1.1 200` response (`/tokenize`: fixed JSON; `/completion`: SSE body written in configurable byte slices with `flush()` between them). Tests:

```rust
#[test] fn count_tokens_uses_tokenize()          // stub returns 6 tokens → Ok(6)
#[test] fn prompt_tokens_counts_formatted_turn()  // stub records request body; assert it contains "<|turn>user\nhi\n<turn|>\n<|turn>model\n"
#[test] fn context_is_params_cap()                // context_tokens() == 8192
#[test] fn generate_streams_tokens_and_finishes() // SSE "po","ng",stop → Ok(vec!["pong"]); events: Token{0,"po"},Token{0,"ng"},Finished{0}
#[test] fn events_split_across_chunks()           // same SSE body written 3 bytes at a time, incl. a multi-byte "é" → Ok(vec!["poné"])
#[test] fn batch_returns_in_request_order()       // 2 requests; stub answers prompt B before A → Ok(vec![a_text, b_text])
#[test] fn cancel_stops_generation()              // cancel set from on_event after first Token; stub would stream forever → Err(LlmError::Cancelled) within 2s
#[test] fn unreachable_server_is_engine_error()   // base "http://127.0.0.1:1" → Err(LlmError::Engine(msg)) with msg containing "127.0.0.1:1"
#[test] fn completion_body_carries_params()       // recorded body has "n_predict":<max_tokens>, "temperature":0.2, "top_p":0.9, "stream":true
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --features remote-llm llm::remote`
Expected: compile error / FAIL (`RemoteSummarizer` not defined).

- [ ] **Step 3: Implement `RemoteSummarizer`**

- `count_tokens(text)` → `/tokenize` with `text`; `prompt_tokens(p)` → same with `format_gemma4_user_prompt(p)`; `context_tokens()` → `params.context_tokens as usize`.
- `generate`: inside one `tauri::async_runtime::block_on`, spawn one task per request (`tauri::async_runtime::spawn`) that POSTs `/completion` and pushes `(seq, Piece::Text(String) | Piece::Done | Piece::Err(String))` into a `std::sync::mpsc::channel` (tokio is not a direct dep; don't add it). Do not wrap `generate` in `block_on`: the spawned tasks run on tauri's runtime while the calling worker thread (a plain std thread, `jobs/queue.rs:137`) drains the channel with `recv_timeout(50ms)`, appends text per `seq`, calls `on_event`, and checks `cancel` after each piece; on cancel, `abort()` every task handle and return `Cancelled`. Return when every `seq` is `Done`.
- SSE parsing: keep a `Vec<u8>` buffer per stream; split on `b"\n\n"`; for each event strip `data: `, `serde_json::from_slice`; only convert complete events to `String` (this is what makes split UTF-8 safe).
- Errors: any reqwest / non-2xx / bad JSON → `LlmError::Engine(format!("{url}: {e}"))`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --features remote-llm llm::remote`
Expected: 9 passed. Also run `cargo test --manifest-path src-tauri/Cargo.toml` (no feature): all existing tests pass, `remote` not compiled.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/src/llm/mod.rs src-tauri/src/llm/llama.rs src-tauri/src/llm/remote.rs
git commit -m "feat(llm): remote-llm feature with llama-server summarizer for VM tests"
git push -u origin HEAD
```

---

### Task 2: `RemoteLoader` and selection in `lib.rs`

**Files:**
- Modify: `src-tauri/src/llm/remote.rs` (add loader + `remote_url`)
- Modify: `src-tauri/src/lib.rs:59-62` (`summarizer_loader`, non-fake branch)

**Interfaces:**
- Consumes: `RemoteSummarizer::new(base: &str, params: llama::Params) -> Result<Self>`; `llama::LlamaLoader::new(Arc<ModelManager>)`; `llama::params(&str) -> Params`.
- Produces: `pub struct RemoteLoader { local: LlamaLoader, base: String }`, `RemoteLoader::new(models: Arc<ModelManager>, base: String) -> Self`; `impl SummarizerLoader` where `model_id`/`ready` delegate to `local`, and `load(id)` returns `NotReady` when `!self.ready()`, else `Arc::new(RemoteSummarizer::new(&self.base, params(id))?)`.
- Produces: `pub fn remote_url(env: Option<String>) -> Option<String>` (None for unset or empty).

- [ ] **Step 1: Write failing test**

```rust
#[test] fn remote_url_reads_env() {
    assert_eq!(remote_url(None), None);
    assert_eq!(remote_url(Some(String::new())), None);
    assert_eq!(remote_url(Some("http://h:5801".into())), Some("http://h:5801".into()));
}
```

- [ ] **Step 2: Run it to verify it fails** — `cargo test --manifest-path src-tauri/Cargo.toml --features remote-llm remote_url_reads_env` → FAIL (not defined).

- [ ] **Step 3: Implement** `remote_url`, `RemoteLoader`, and in `lib.rs` non-fake `summarizer_loader`:

```rust
#[cfg(feature = "remote-llm")]
if let Some(url) = llm::remote::remote_url(std::env::var("SCRIBULAR_LLM_URL").ok()) {
    // log via the existing diag/log facility at INFO: "SCRIBULAR_LLM_URL set: summaries use {url}"
    return Arc::new(llm::remote::RemoteLoader::new(Arc::clone(models), url));
}
```

- [ ] **Step 4: Verify**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --features remote-llm llm::remote` → all pass.
Run: `cargo clippy --manifest-path src-tauri/Cargo.toml --features remote-llm -- -D warnings` and the same without the feature → clean.

- [ ] **Step 5: Commit + push** — `git commit -m "feat(llm): select remote summarizer from SCRIBULAR_LLM_URL in remote-llm builds"`; `git push`.

---

### Task 3: uitest forwarding, docs, real VM run

**Files:**
- Modify: `tools/uitest/Sources/uitest/AppLifecycle.swift:54-55`
- Modify: `tools/uitest/README.md` (new "Host llama-server (VM)" section)
- Modify: `CLAUDE.md` (E2E section: one line pointing to that README section)

**Interfaces:**
- Consumes: env var `SCRIBULAR_LLM_URL` read by the app (Task 2).

- [ ] **Step 1: Forward the env var** — after `let configuration = NSWorkspace.OpenConfiguration()`:

```swift
if let url = ProcessInfo.processInfo.environment["SCRIBULAR_LLM_URL"], !url.isEmpty {
    configuration.environment = ["SCRIBULAR_LLM_URL": url]
}
```

Run: `swift build --package-path tools/uitest` → builds. `swift test --package-path tools/uitest` → passes.

- [ ] **Step 2: README section** — host command `llama-server -m gemma-4-E2B-it-GGUF-Q4_K_M.gguf --host 192.168.64.1 --port 5801 -np 4`; why `--host` (default loopback is unreachable from the VM; Softnet already allows `192.168.64.1/32`; not `0.0.0.0`, which exposes the unauthenticated server to the LAN); VM check `curl http://192.168.64.1:5801/health`; build `APPLE_SIGNING_IDENTITY="Scribular Local Dev" TAURI_SIGNING_PRIVATE_KEY=... npm run tauri -- build --bundles app --features remote-llm`; run `SCRIBULAR_LLM_URL=http://192.168.64.1:5801 swift run --package-path tools/uitest uitest <scenario> --app <path>`; note: same GGUF on host as the app manifest, or results won't match on-device.

- [ ] **Step 3: Real run on the VM**

Build as above, then run `as11-note-style-default` with `SCRIBULAR_LLM_URL=http://192.168.64.1:5801`.
Expected: scenario passes; app log contains `SCRIBULAR_LLM_URL set: summaries use http://192.168.64.1:5801`; host llama-server log shows `/completion` requests. Record wall time vs. a previous local-model run if one is in the logs.

- [ ] **Step 4: Commit + push** — `git commit -m "test(uitest): forward SCRIBULAR_LLM_URL to the app; document host llama-server"`; `git push`.
