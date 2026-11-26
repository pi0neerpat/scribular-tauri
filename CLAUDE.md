## E2E / uitest

- Verify runs use a debug build. For a fresh target directory, follow the native library setup in [Build and run](tools/uitest/README.md#build-and-run). Build with `APPLE_SIGNING_IDENTITY="Scribular Local Dev" npm run tauri -- build --debug --bundles app --config tools/uitest/tauri-debug-overlay.json`. `launch` trusts each rebuild's keychain access automatically (`tools/uitest/trust-build.sh`); if it fails with "no login password", ask the user to run `bash tools/uitest/trust-build.sh --store-password` once. First time on a Mac: `bash tools/uitest/setup-signing.sh`, then answer the keychain and Screen & System Audio Recording prompts once by hand.
- Run `SCRIBULAR_SKIP_LICENSE=1 swift run --package-path tools/uitest uitest <scenario> --app <path to Scribular.app>` from the repo root. Release builds ignore `SCRIBULAR_SKIP_LICENSE`.
- Concurrent uitest runs queue on the desktop lock for up to 30 minutes; use `--lock-timeout 0` to refuse immediately.
- Launch answers Scribular keychain prompts with the stored login password and fails within 15 seconds with a fix command if it cannot; trust-build output is saved in the run folder.
- Recording is opt-in: use `--record <NN-slug-happy-path>` only for a happy path or an issue that cannot be reproduced reliably. It writes an MP4 and JSON metadata; failed runs use `-FAILED.mp4`. Recordings are not committed.
- Recorded runs pace UI actions by 2.5s by default; use `--pace 2s` to override, or `--pace 0` to disable. Scripts can use `pause 3s` for read time and `pace 2s` to change pacing.
- Recording tests use App audio + `play "fixture.wav"`, never the microphone. Start from `tools/uitest/scenarios/system-audio-smoke.uitest`.
- Assert phrases from the fixture's transcript, never summary wording (the LLM output varies).
- Models come from `~/Library/Caches/scribular-uitest/models`. Never delete it to fix a failing test: that is a 1.85 GB re-download, not a different result.
- In the Tart VM, uitest uses the host llama-server automatically (debug overlay enables `remote-llm`; uitest probes `192.168.64.1:5801`). Set `SCRIBULAR_LLM_URL=` (empty) to force on-device. See "Host llama-server (VM)" in `tools/uitest/README.md`.
- Details: `tools/uitest/README.md`.
