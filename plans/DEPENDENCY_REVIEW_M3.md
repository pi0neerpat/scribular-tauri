# Dependency review: M3 (model download)

**Date:** 2026-11-08
**Status:** Original M3 review approved 2026-10-19; Gemma 4 E2B model choice approved by Patrick 2026-11-07 (see `2026-11-07-note-style-coach-spec.md`, execution notes).
**Policy:** `../notes-assistant-electron/CLAUDE.md` § Dependency Policy (review, audit,
exact pin, in-house check).

**Version rule used:** the M3 downloader dependencies use the 2026-10-05 cutoff. The
AS10 inference-engine follow-up uses the 2026-10-25 cutoff (the 2026-11-08 date minus
two weeks); its separate package and build-script review is in
[`DEPENDENCY_REVIEW_ENGINES.md`](DEPENDENCY_REVIEW_ENGINES.md).

---

## Rust crates

| Crate | Pin | Published | Recent DL (90 d) | Owners | License | OSV / RustSec |
|---|---|---|---|---|---|---|
| `reqwest` (`default-features = false`, `native-tls`) | `=0.13.5` | 2026-09-08 | 220 M | seanmonstar | MIT / Apache-2.0 | clean |
| `sha2` | `=0.10.9` | already locked | (RustCrypto) | RustCrypto org | MIT / Apache-2.0 | clean |
| `llama-cpp-2` | `=0.1.160` | 2026-10-15 | 0.80 M | MarcusDunn | MIT OR Apache-2.0 | clean |
| `llama-cpp-sys-2` | `=0.1.160` | 2026-10-15 | (same project) | MarcusDunn | MIT OR Apache-2.0 | clean |

The two llama crates are the AS10 inference-engine pins. The crates.io metadata
recheck on 2026-11-08 confirmed their publish dates and licenses; OSV reported no
advisories for either exact version. Patrick approved these pins in the 2026-11-07
spec decision. Their feature, vendored-code, build-script and runtime review is kept
in `DEPENDENCY_REVIEW_ENGINES.md`.

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

The required Whisper and summary-model files are public and ungated: anonymous requests
work, and both redirect (302) to Hugging Face's CDN, which answers `Range` requests with
`206`. URLs are pinned to a repo commit, not `main`, so the file under a URL can't
change. The app also checks size and sha256 before renaming the download into place.
The optional speaker model is hosted separately and is not covered by these checks.
Anonymous one-byte range requests to each required pinned URL returned `206` after one
redirect.

| Model | URL (pinned commit) | Size (bytes) | sha256 | License |
|---|---|---|---|---|
| Whisper base | `huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-base.bin` | 147 951 465 | `60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe` | MIT |
| Gemma 4 E2B IT Q4_K_M | `huggingface.co/axteam/gemma-4-E2B-it-GGUF/resolve/21d0b2296fd0fd4726c83e3015b086add5fa99d8/gemma-4-E2B-it-GGUF-Q4_K_M.gguf` | 3 427 879 936 | `0f1a91a281de002438842f54b4a186d7ea913be290c22ba2d10314e67e0e193a` | Apache-2.0 |

Hugging Face's pinned-revision file metadata at
`https://huggingface.co/api/models/axteam/gemma-4-E2B-it-GGUF/tree/21d0b2296fd0fd4726c83e3015b086add5fa99d8?recursive=false`
reports size
`3,427,879,936` and LFS SHA-256
`0f1a91a281de002438842f54b4a186d7ea913be290c22ba2d10314e67e0e193a`, matching the
manifest. A stream-only download from the pinned URL also read exactly
`3,427,879,936` bytes and produced that SHA-256; no local model file was retained. AS10
still covers installing through the app and running inference.
The axteam model card describes the GGUF as a quantized derivative governed by Gemma 4's
license. Google's [Gemma 4 model card](https://ai.google.dev/gemma/docs/core/model_card_4)
lists Apache 2.0, and the manifest links to Google's [Apache 2.0 license](https://ai.google.dev/gemma/apache_2).

---

## Recommendation

| Group | Recommendation |
|---|---|
| `reqwest =0.13.5` (no defaults, `native-tls`) | ✅ Approve |
| `sha2 =0.10.9` | ✅ Approve (already in the tree) |
| Model pins above | ✅ Approved; Gemma 4 E2B choice approved by Patrick in the spec on 2026-11-07 |

## Lockfile check

A trial resolve (M1 + M3 crates on a copy of `src-tauri`) found two crates past the
cutoff, both moved with `cargo update --precise`:

| Crate | Resolved | Pinned |
|---|---|---|
| `schannel` (Windows only) | 0.1.30 (2026-10-16) | 0.1.29 (2026-03-10) |
| `zeroize` | 1.9.1 (2026-10-06) | 1.9.0 (2026-06-12) |

The historical full whole-lockfile recheck below covers the 2026-10-05 cutoff. The
2026-11-08 follow-up against the 2026-10-25 cutoff is recorded in
[`DEPENDENCY_REVIEW_ENGINES.md`](DEPENDENCY_REVIEW_ENGINES.md): the later scan checked
572 crates and found seven post-cutoff packages, all already present at the same
versions in `origin/main`. The two llama 0.1.160 pins added here meet the cutoff.

**Recheck (2026-10-19, real `src-tauri/Cargo.lock` after U4):** 446 crates checked,
0 published after the 2026-10-05 cutoff.
