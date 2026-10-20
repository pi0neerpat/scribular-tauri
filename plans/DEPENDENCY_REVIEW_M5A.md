# Dependency review: M5a (recording, U8)

**Date:** 2026-10-20
**Status:** Approved 2026-10-20.
**Policy:** `../notes-assistant-electron/CLAUDE.md` § Dependency Policy (review, audit,
exact pin, in-house check).

**Version rule used:** the newest stable release published on or before 2026-10-06
(two weeks before today).

---

## Rust crates

| Crate | Pin | Published | Recent DL (90 d) | Owners | License | OSV / RustSec |
|---|---|---|---|---|---|---|
| `cpal` (default features, none enabled) | `=0.18.2` | 2026-08-16 | 8.7 M | tomaka, RustAudio `cpal-maintainers` team | Apache-2.0 | clean |

**Notes**
- **cpal is the microphone path only.** It opens the default input device through
  CoreAudio and hands the callback interleaved samples at the device's own rate. The
  ring buffer, the resampler to 16 kHz mono, and the chunk writer are in-house
  (`src-tauri/src/audio/`), as the plan says.
- **No optional features.** `asio`, `jack`, `audioworklet` and friends stay off. cpal's
  default feature set is empty.
- **`tauri-plugin-shell` is not added (deviation from KTD11's wording).** The plugin is
  only a way to spawn the `externalBin` sidecar. Tauri's bundler places `externalBin`
  binaries next to the main executable whether or not the plugin is present, and
  `tauri-build` copies them next to the dev binary too. The app spawns
  `current_exe().parent()/AudioCapCLI` with `std::process::Command` from Rust. That
  saves a dependency tree (the plugin pulls `open`, `shared_child`, `os_pipe`,
  `regex`, `encoding_rs`) and, more importantly, keeps any shell permission out of the
  webview: `capabilities/default.json` needs no `shell:*` entry. Also, the newest 2.x
  plugin releases (2.4.1 on 2026-10-07, 2.5.0 on 2026-10-10) are inside the two-week
  window, so we'd be stuck on 2.4.0.
- **No npm packages.** The frontend talks to Rust commands only;
  `@tauri-apps/plugin-shell` is not needed.

**New crates in the lockfile** (trial resolve on a copy of `src-tauri`)

| Built on macOS | Other targets only |
|---|---|
| `cpal 0.18.2` (2026-08-16), `coreaudio-rs 0.14.2` (2026-04-29), `objc2-audio-toolbox 0.3.2`, `objc2-core-audio 0.3.2`, `objc2-core-audio-types 0.3.2` (all 2025-10-04), `dasp_sample 0.11.0` (2020-05-29), `mach2 0.6.0` (2025-11-16) | `alsa 0.11.0`, `alsa-sys 0.4.0` (Linux), `jni 0.22.4`, `jni-macros 0.22.4`, `num-derive 0.4.2`, `simd_cesu8 1.2.0`, `simdutf8 0.1.5` (Android), `objc2-avf-audio 0.3.2` (iOS) |

`objc2`, `objc2-foundation`, `objc2-core-foundation`, `block2`, `dispatch2`, `libc`
and `bitflags` were already in the macOS build through Tauri. Every new crate was
published before the cutoff, so no `--precise` downgrades were needed.

## build.rs review

- **`cpal`** (10 lines). Reads `CPAL_ASIO_DIR` and sets an `asio` cfg if it's present
  (Windows ASIO only). No process spawning, no network, no file writes.
- `coreaudio-rs`, `objc2-audio-toolbox`, `objc2-core-audio`, `objc2-core-audio-types`,
  `dasp_sample`, `mach2`: no build.rs. They link the system CoreAudio and AudioToolbox
  frameworks through `#[link]` attributes.

**Audit:** OSV reports no advisories for `cpal 0.18.2` or `coreaudio-rs 0.14.2`.

**In-house:** a CoreAudio input unit (AUHAL setup, device enumeration, format
negotiation, the render callback) is a few hundred lines of unsafe FFI. That is not a
reasonable in-house job for a prototype. Everything after the callback is in-house.

---

## Bundled binary: AudioCapCLI (KTD11)

Copied unchanged from the Electron app, which ships it today.

| Field | Value |
|---|---|
| Source | `../notes-assistant-electron/src/service/runners/AudioCapCLI` |
| Bundled as | `src-tauri/binaries/AudioCapCLI-aarch64-apple-darwin` (`bundle.externalBin`) |
| sha256 | `d07fec43759f4ba05a6ab0ea0f4f19e89a550701b38058d0da22505f3193353b` |
| Size | 417 936 bytes, universal (x86_64 + arm64) |
| Signature | Developer ID, team `5KDQN87JD2`, hardened runtime, signed 2025-12-26 |
| Interface | `--list-sources` prints `name\|<rate> Hz, <n> ch\|<base64 png>` lines; `--source <name>` streams interleaved f32 little-endian PCM at that format on stdout; `Audio recording permission denied.` on stderr when TCC refuses |

The binary is opaque to this review (no source in either repo). It was already
approved for the Electron app; this just reuses that approval. It makes no network
requests in the Electron app's traffic, and the U8 Wi-Fi-off check covers it again.

---

## Recommendation

| Group | Recommendation |
|---|---|
| `cpal =0.18.2` | ✅ Approve |
| `tauri-plugin-shell` | ❌ Not needed: spawn the sidecar with `std::process::Command` |
| AudioCapCLI sidecar, sha256 above | ✅ Approve (same binary the Electron app ships) |

## Lockfile check

Trial resolve: 15 crates added, all published before 2026-10-06. The full
whole-lockfile recheck is rerun on the real `Cargo.lock` when U8 lands the change, and
its result is appended here.

Recheck on the committed `Cargo.lock` (2026-10-20, cutoff 2026-10-06T23:59:59, script
from `docs/solutions/workflow-issues/lockfile-publish-date-cutoff-check.md`):
483 registry crates checked, 0 late. The 15 not in `main`'s lockfile are `cpal`
0.18.2 and its tree: `alsa` 0.11.0, `alsa-sys` 0.4.0, `coreaudio-rs` 0.14.2,
`dasp_sample` 0.11.0, `jni` 0.22.4, `jni-macros` 0.22.4, `mach2` 0.6.0,
`num-derive` 0.4.2, `objc2-audio-toolbox` 0.3.2, `objc2-avf-audio` 0.3.2,
`objc2-core-audio` 0.3.2, `objc2-core-audio-types` 0.3.2, `simd_cesu8` 1.2.0,
`simdutf8` 0.1.5. `package-lock.json` is unchanged.
