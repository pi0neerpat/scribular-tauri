# Dependency review: M3 (model download)

**Date:** 2026-10-19
**Status:** Approved 2026-10-19.
**Policy:** `../notes-assistant-electron/CLAUDE.md` § Dependency Policy (review, audit,
exact pin, in-house check).

**Version rule used:** the newest stable release published on or before 2026-10-05.

---

## Rust crates

| Crate | Pin | Published | Recent DL (90 d) | Owners | License | OSV / RustSec |
|---|---|---|---|---|---|---|
| `reqwest` (`default-features = false`, `native-tls`) | `=0.13.5` | 2026-09-08 | 220 M | seanmonstar | MIT / Apache-2.0 | clean |
| `sha2` | `=0.10.9` | already locked | (RustCrypto) | RustCrypto org | MIT / Apache-2.0 | clean |

**Notes**
- **reqwest without default features.** The 0.13 defaults use rustls with `aws-lc-rs`,
  which compiles a large C/assembly library through CMake. `native-tls` on macOS uses
  the system TLS (Security.framework) instead: nothing extra to compile, and system
  certificate trust.
- The `stream` feature isn't needed. The downloader reads with `Response::chunk()` in a
  loop. Tauri already brings tokio with the multi-threaded runtime.
- **sha2 stays at 0.10.9**, which `tauri-codegen` already pulls in. 0.11.0 would add a
  second copy for no gain, and 0.11.1 is past the cutoff.
- **`hex` is not added.** Hex-encoding a 32-byte digest is a 3-line `format!` loop.
- **Test server is in-house.** `tests/models_download.rs` uses a small
  `std::net::TcpListener` server that serves a file, supports `Range`, and can cut the
  connection mid-body. That avoids `wiremock`/`httpmock` and their trees.

**New crates in the lockfile**

| Built on macOS | Other targets only |
|---|---|
| `reqwest` (macOS was previously not compiling it), `hyper-tls 0.6.0`, `tokio-native-tls 0.3.1`, `native-tls 0.2.18`, `security-framework 3.7.0`, `security-framework-sys 2.17.0`, `rustls-pki-types 1.15.1`, `tempfile 3.27.0`, `rustix 1.1.5`, `errno 0.3.14`, `zeroize 1.9.0` | `openssl 0.10.81`, `openssl-sys 0.9.117`, `openssl-macros 0.1.1`, `openssl-probe 0.2.1`, `foreign-types 0.3.2`, `foreign-types-shared 0.1.1`, `linux-raw-sys 0.12.1`, `schannel 0.1.29` |

`reqwest`, `hyper 1.11.1` and `hyper-util 0.1.21` were already in `Cargo.lock` for
Tauri's other targets; on macOS they now compile.

## build.rs review

- **`rustix`** (281 lines). Runs `rustc` probes (`--emit=metadata` on tiny snippets) to
  detect compiler features. Spawns rustc only; no network, no writes outside `OUT_DIR`.
- **`native-tls`** (29 lines). Reads `DEP_OPENSSL_VERSION_NUMBER` (unset on macOS) to set
  a cfg.
- `reqwest`, `hyper-tls`, `tokio-native-tls`, `security-framework(-sys)`, `tempfile`,
  `errno`, `zeroize`: no build.rs. (`openssl-sys` has one but isn't built on macOS.)

**Audit:** OSV reports no advisories for any of the crates above at these versions,
including `hyper 1.11.1` and `hyper-util 0.1.21`.

**In-house:** HTTPS with redirects (both hosts below redirect to a CDN) is not
reasonable to write in-house.

---

## Models (KTD8 manifest)

Both files are public and ungated: anonymous requests work, and both redirect (302) to
Hugging Face's CDN, which answers `Range` requests with `206`. URLs are pinned to a repo
commit, not `main`, so the file under a URL can't change. The app also checks size and
sha256 before renaming the download into place.

| Model | URL (pinned commit) | Size (bytes) | sha256 | License |
|---|---|---|---|---|
| Whisper base | `huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-base.bin` | 147 951 465 | `60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe` | MIT |
| Gemma 2 2B IT Q4_K_M | `huggingface.co/bartowski/gemma-2-2b-it-GGUF/resolve/855f67caed130e1befc571b52bd181be2e858883/gemma-2-2b-it-Q4_K_M.gguf` | 1 708 582 752 | `e0aee85060f168f0f2d8473d7ea41ce2f3230c1bc1374847505ea599288a7787` | Gemma Terms of Use |

⚠️ **Gemma is not under an OSI license.** The Gemma Terms of Use allow redistribution
and commercial use but carry a prohibited-use policy that passes on to users. The
manifest records the license name and URL, and the download UI should show it. This
matches the model choice already in the plan; flagging it so the approval covers it.

---

## Recommendation

| Group | Recommendation |
|---|---|
| `reqwest =0.13.5` (no defaults, `native-tls`) | ✅ Approve |
| `sha2 =0.10.9` | ✅ Approve (already in the tree) |
| Model pins above | ✅ Approve, with the Gemma license shown in the UI |

## Lockfile check

A trial resolve (M1 + M3 crates on a copy of `src-tauri`) found two crates past the
cutoff, both moved with `cargo update --precise`:

| Crate | Resolved | Pinned |
|---|---|---|
| `schannel` (Windows only) | 0.1.30 (2026-10-16) | 0.1.29 (2026-03-10) |
| `zeroize` | 1.9.1 (2026-10-06) | 1.9.0 (2026-06-12) |

The full whole-lockfile recheck is rerun on the real `Cargo.lock` when U2/U4 land the
change, and its result is appended here.

**Recheck (2026-10-19, real `src-tauri/Cargo.lock` after U4):** 446 crates checked,
0 published after the 2026-10-05 cutoff.
