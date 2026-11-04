# U11 Release Check: Signed, Notarized DMG (R9, R14)

- **Result:** Pass, 2026-11-03.
- **Build:** commit `b45a066`, GitHub Actions run [41748339411](https://github.com/pi0neerpat/scribular-tauri/actions/runs/41748339411) (`.github/workflows/release-mac.yml`, `macos-latest`, arm64).
- **Artifact:** `Scribular_0.1.0_aarch64.dmg`, 10.64 MiB (11,159,985 bytes), sha256 `e8f00cbe42b018e73fa650e7392f97f9d9605afed49c85b10d2ad356ac0e602e`.
- **Size vs Electron:** the Electron v1.5.9 `.app` is 874 MB. U12 records the like-for-like download size.

## Verification

| Check | Result |
|---|---|
| `codesign --verify --deep --strict` on the app | Pass (CI and the downloaded DMG) |
| `codesign --verify --strict` on the AudioCapCLI sidecar, Developer ID authority | Pass (CI) |
| `spctl -a -vv` | `accepted`, `source=Notarized Developer ID`, team 5KDQN87JD2 |
| `xcrun stapler validate` on the app | Pass |
| Every Mach-O in `Contents/MacOS` and `Contents/Frameworks` links only `@rpath`, `@executable_path`, `@loader_path`, `/usr/lib` or `/System` | Pass |
| A second Mac without Homebrew OpenSSL launches the app | Pass |

## Incident: the first notarized build crashed at launch

- The first green run (on `main`, `40e7884`) produced a DMG that aborted in dyld on the second Mac: `Library not loaded: /opt/homebrew/opt/openssl@4/lib/libssl.4.dylib`, referenced from `Contents/Frameworks/libllama-common.0.dylib`.
- **Cause:** llama.cpp's `LLAMA_OPENSSL` option defaults to ON. Its vendored cpp-httplib found the runner's Homebrew OpenSSL and linked it by absolute path. Signing and notarization do not check whether a linked library exists.
- **Fix:** `CMAKE_DISABLE_FIND_PACKAGE_OpenSSL = "ON"` in `.cargo/config.toml`. llama-cpp-sys-2 forwards `CMAKE_*` env vars to CMake. The app never uses llama.cpp's HTTP client, and model downloads go through `reqwest`.
- **Guard:** the release workflow now fails if any bundled binary links outside the bundle or the OS.
- **Lesson:** the second-Mac retest first ran the old install still in `/Applications`. Compare `dwarfdump --uuid` on `libllama-common.0.dylib` with the build before you trust a retest.
