# AudioCapCLI produces no PCM when launched with null stdin

Date: 2026-11-02

## Symptom

Microphone recordings work, but app-audio recordings save no chunks and end with
`no audio was recorded`. The same helper works in notes-assistant-electron.

## Comparison and cause

Electron uses `child_process.spawn(helper, ["--source", sourceId])`, whose default
stdio includes a live stdin pipe. Tauri used `Command` with `stdin(Stdio::null())`.
Source names, little-endian interleaved Float32 decoding and format parsing agree.
The Electron source, packaged Electron and Tauri helper binaries all have SHA-256
`d07fec43759f4ba05a6ab0ea0f4f19e89a550701b38058d0da22505f3193353b`.

On the Mac VM, simultaneous captures of a local WAV played by `afplay`:

| Launcher | Helper stdin | PCM bytes | RMS |
| --- | --- | ---: | ---: |
| Node child_process.spawn | open pipe | 3,350,528 | not measured |
| Node child_process.spawn | ignore/null | 0 | 0 |
| Python subprocess, stdout drained with selectors | open pipe | 2,195,456 | 0.1111 |
| Python subprocess, stdout drained with selectors | null | 0 | 0 |

The independent Python comparison rules out Node socket-backed stdout being the
reason for the difference. No PCM or speech text was retained. macOS unified logs
reported granted audio permission and 48 kHz stereo interleaved Float32, but stderr
was empty even in the failing case. These results establish the helper's stdin
contract; its opaque internal implementation was not inspected.

## Fix

Use `stdin(Stdio::piped())` for capture and retain `Child.stdin` for the entire
capture lifetime. Do not take/drop it or use `wait_with_output`/`communicate` in a
probe that closes stdin before capture. Listing sources may still use null stdin.
Dropping `CliCapture` kills and waits for the helper and joins its readers.

## Verification

The unit regression models a helper that requires piped stdin, emits known f32
samples, and checks delivery plus sink closure on drop. Real-helper verification
must additionally play the checked-in WAV on the same Mac and assert nonzero PCM.
A live helper and successful source enumeration alone do not establish capture.

The existing 500 ms startup grace still permits a live helper with no delivered
samples; changing that policy is separate from this launch-contract repair.

### Results on the repaired adapter

- New regression failed on the original code with `stdin must stay piped`.
- `cargo test --lib audio::`: 18 passed, 1 opt-in native test ignored.
- Explicit native test `audio::system::tests::bundled_helper_delivers_audio_during_playback`
  passed: 577,536 finite samples, 48 kHz stereo, RMS 0.09837, with the real bundled
  helper feeding the Rust adapter and sample ring. This is VM capture verification,
  not a physical-host UI/transcript test.
- `cargo build --bin scribular`: passed (dev profile).

Commands used `CARGO_TARGET_DIR=/Users/admin/cargo-target/scribular-tauri`,
`CARGO_BUILD_JOBS=2`, `CARGO_INCREMENTAL=0`, `CARGO_PROFILE_DEV_DEBUG=0`, and
`CARGO_PROFILE_TEST_DEBUG=0`, with `--manifest-path src-tauri/Cargo.toml`.
The first build attempt reached its 60-second timeout while compiling dependencies;
the resumed compile completed. No models were downloaded and no permissions changed.

Restart/rebuild the Tauri app to use the repaired launch settings. Existing packaged
apps still contain their old code until rebuilt. User-staged package-lock and Tauri
configuration edits were left intact.
