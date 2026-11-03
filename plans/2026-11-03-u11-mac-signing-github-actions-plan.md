# U11: Signed, notarized macOS build via GitHub Actions

Implements U11 of `plans/2026-10-19-2001-feat-tauri-rewrite-evidence-prototype-plan.md`.
The CI flow replaces the planned local `scripts/release-mac.sh`.

## Reference: the Electron flow (notes-assistant-electron)

- `.github/workflows/build-electron.yml`: `workflow_dispatch`. It bumps the patch version, imports the
  Developer ID `.p12` into a temporary keychain, runs `electron-builder` with
  `hardenedRuntime: true`, notarizes with `notarytool` (in `hooks/afterSign.js`) and staples,
  then publishes a draft release to `pi0neerpat/scribular-releases`.
- Secrets: `APPLE_CERTIFICATE_P12_BASE64`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_IDENTITY`,
  `APPLE_ID`, `APPLE_ID_PASSWORD` (app-specific password), `APPLE_TEAM_ID`, `PERSONAL_ACCESS_TOKEN`.
- Entitlements (`build/entitlements.mac.plist`): audio-input, allow-jit,
  allow-unsigned-executable-memory, network.client.

## Tauri equivalent

Tauri v2's bundler signs, notarizes and staples by itself when these env vars are set.
That replaces the keychain step and the afterSign hook.

| Tauri env var | From the existing secret |
|---|---|
| `APPLE_CERTIFICATE` | `APPLE_CERTIFICATE_P12_BASE64` |
| `APPLE_CERTIFICATE_PASSWORD` | `APPLE_CERTIFICATE_PASSWORD` |
| `APPLE_SIGNING_IDENTITY` | `APPLE_IDENTITY` |
| `APPLE_ID` | `APPLE_ID` |
| `APPLE_PASSWORD` | `APPLE_ID_PASSWORD` |
| `APPLE_TEAM_ID` | `APPLE_TEAM_ID` |

## Changes

1. `src-tauri/entitlements.plist`: `com.apple.security.device.audio-input` and
   `com.apple.security.network.client` (for model downloads). Leave out the JIT entitlements
   at first, because they exist for V8. Add them only if notarization or launch fails.
2. `src-tauri/tauri.conf.json`: `bundle.targets` gets `["app", "dmg"]`, and
   `bundle.macOS.entitlements` is set to `"entitlements.plist"`. Hardened runtime is already
   on by default in v2.
3. `.github/workflows/release-mac.yml`:
   - `workflow_dispatch` only, on `macos-latest` (arm64). This build is arm64-only because the
     sidecar is `AudioCapCLI-aarch64-apple-darwin` and llama uses Metal.
   - Steps: checkout, setup-node, rust toolchain, `npm ci`, `npm run tauri build`
     with the env mapping above.
   - Verify: `codesign --verify --deep --strict` on the `.app` and on the sidecar,
     `spctl -a -vv` (expects "Notarized Developer ID"), `xcrun stapler validate`.
   - Upload the DMG as a workflow artifact and print its size. The size goes into
     `plans/evidence/release.md`.
4. `plans/evidence/release.md`: record the run URL, the verify output and the DMG size.

## Skipped (not needed for U11)

Version bumping, publishing to `scribular-releases`, Intel and Windows builds, and Rust/npm caching.

## Prerequisites (owner: Patrick)

- The 6 secrets have to be set on `pi0neerpat/scribular-tauri`. GitHub secrets can't be
  read back out of the Electron repo, so set them with `gh secret set <NAME> -R pi0neerpat/scribular-tauri`.
- The GitHub remote has to work locally (see Risks).

## Risks

- llama is built with `dynamic-link`. Its dylibs must be bundled into `Contents/Frameworks`
  and signed. If they aren't, notarization rejects the app or the app fails to launch on a
  clean Mac. The first CI run will show which.
- Whether Tauri re-signs `externalBin` sidecars with the hardened runtime needs to be confirmed with
  `codesign -dv --verbose=4` on the bundled AudioCapCLI.
- The final U11 check (a second Mac opens the app with no Gatekeeper prompt) has to be done by hand.
