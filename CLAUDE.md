## E2E / uitest

- Build the app under test with `APPLE_SIGNING_IDENTITY="Scribular Local Dev" npm run tauri -- build --bundles app`. First time on a Mac: `bash tools/uitest/setup-signing.sh`, then answer the keychain and Screen & System Audio Recording prompts once by hand.
- Run: `swift run --package-path tools/uitest uitest <scenario> --app <path to Scribular.app>` from the repo root.
- Recording tests use App audio + `play "fixture.wav"`, never the microphone. Start from `tools/uitest/scenarios/system-audio-smoke.uitest`.
- Assert phrases from the fixture's transcript, never summary wording (the LLM output varies).
- Models come from `~/Library/Caches/scribular-uitest/models`. Never delete it to fix a failing test: that is a 1.85 GB re-download, not a different result.
- Details: `tools/uitest/README.md`.
