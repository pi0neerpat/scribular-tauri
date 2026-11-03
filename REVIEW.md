# Review instructions — scribular-tauri

Knightwatch reads this file from `main` and adds it to every review agent's
prompt, so keep it short. Edit it when the product scope or architecture changes.

## Product context

- **What it is:** a Tauri 2 desktop app (macOS Apple Silicon first) that records
  a session (mic, system/app audio, or both), transcribes it live on-device with
  whisper, labels `Client:` / `You:` speakers, and summarizes it on-device with a
  llama model and a prompt template. Users are clinicians, so transcripts are
  sensitive health data.
- **Stage:** pre-PMF prototype testing a rewrite of the Electron app
  (`notes-assistant-electron`). Single operator, fewer than 10 users. There is no
  server, no account system and no external API consumers.
- **Optimize for:** iteration speed and fewer lines of code. A handled edge case
  nobody asked for is a cost, not a feature.

## Architecture

- `src-tauri/src/` (Rust): `audio/` capture and resampling, `stt/` whisper
  streaming, `llm/` llama summarization, `jobs/` session and summary queue,
  `models/` first-run model download, `store/` SQLite (sessions, prompts,
  settings, migrations), `commands.rs` IPC commands.
- `src/` (Svelte 5 + TypeScript): `lib/api.ts` is the only IPC wrapper;
  `lib/components/` holds the views. Rust types reach TS through `ts-rs`.
- Everything runs in-process; nothing leaves the machine except model downloads.

## What to look for

Most bugs here sit on these seams:

1. **Rust ↔ Svelte contract:** command and event payloads, `ts-rs` types,
   event-listener ordering vs. initial status snapshots, state the UI derives
   from events that may never fire.
2. **Session and queue state:** `jobs/` state transitions, what survives a
   reload, relaunch or cancel, SQLite migrations.
3. **Audio and model pipelines:** AudioCapCLI subprocess I/O, sample rates,
   chunking cost (see `docs/solutions/`), download retry and verification.
4. **Privacy:** transcript or audio leaking to logs, temp files or the network.
   Model downloads come from the fixed manifest only.

## Out of scope — don't spend effort here

- Scale, multi-user, concurrency beyond one recording at a time, backward
  compatibility with external consumers (there are none).
- Windows and Linux behaviour (later milestone).
- Dependency versions: they are pinned on purpose under the two-week rule in
  `plans/DEPENDENCY_REVIEW_*.md`.
- `plans/` and `docs/` prose, unless the PR changes their meaning.

## Review economy

- Read the diff and the direct callers of what it touches; don't survey the
  whole repo.
- Prefer one concrete bug over several speculative probes. Skip an angle with
  nothing real to report rather than padding it.

## Tests

No `justfile`. Verification is `npm test`, `npm run check`, and `cargo test`
in `src-tauri/`.
