# Dependency review: multi-speaker diarization

**Date:** 2026-11-04  
**Status:** Pending Patrick approval; no dependency or native library has been added.  
**Scope:** First milestone for parity spec 02 only.  
**Policy:** Exact pins; newest stable release at least two weeks old; inspect owners, recent downloads, license, OSV/RustSec, and build scripts before adding anything. Re-check at implementation because this review is time-sensitive.

## Recommendation for approval

Proceed to a post-approval spike with the official `sherpa-onnx` Rust API at `=1.13.8` plus its matching sys crate, if the crate metadata and build script pass the remaining security and supply-chain checks. This is the newest eligible stable version found on 2026-11-04: upstream v1.13.8 was released 2026-09-10, while v2.0.0 (2026-10-23) and v2.0.1 (2026-10-26) are too recent under the two-week policy. The official API documents `SpeakerEmbeddingExtractor`, a stream accepting 16 kHz `f32` audio, `compute`, and the in-memory `SpeakerEmbeddingManager`; the upstream Rust example exercises this API.

This is a proposal, not dependency approval. The dependency is not in `Cargo.toml`/`Cargo.lock`, and its native library has not been downloaded or built. The official Rust API is now a better first spike target than the draft's own-FFI-first ordering, but its prebuilt-library download behavior and application packaging must be inspected and controlled before integration. A source-built C API remains the fallback if the official crate's build or link path fails review. `ort` plus a hand-written frontend is a later fallback only: `ort` latest found was still `2.0.0-rc.15` (2026-10-19), and an in-house fbank implementation would need the same parity evidence.

| Option | Evidence and disposition |
|---|---|
| Official `sherpa-onnx` Rust API | **Preferred spike candidate:** upstream maintained; speaker embedding API exists in v1.13.8; Apache-2.0 project. Pin both wrapper and sys crates exactly after confirming their published metadata. Docs say the build can fetch matching prebuilt native libraries automatically when `SHERPA_ONNX_LIB_DIR` is unset; inspect the exact pinned `build.rs`, disable uncontrolled fetches, and measure bundled/static size before approval. |
| Own thin FFI to sherpa-onnx C API | **Fallback:** C API has speaker embedding extractor and manager calls. It avoids a Rust wrapper dependency but requires project-owned `unsafe`, link configuration, and pinning/hashing sherpa and ONNX Runtime artifacts. No source/build spike was run because that would require adding/downloading native libraries. |
| `sherpa-rs` | **Not preferred:** current docs list 0.6.8 (2025-10-05), the crate includes a separate sys crate and optional prebuilt-library download feature, and its repository now directs users to the official Rust bindings. No security or usage data is claimed here. |
| `ort` + in-house fbank | **Not preferred:** current latest version found is `2.0.0-rc.15` (2026-10-19), still a release candidate; in-house feature extraction adds a parity-sensitive implementation. |
| `candle` / `tract` | **Not evaluated as viable candidates:** no evidence gathered that they cover this model's operators with equivalent behavior; no package should be proposed without a separate spike. |

## Policy review still required before approval

The crate-level review was completed read-only from crates.io metadata and the downloaded **source archives only**; neither package was installed, and no native archive was downloaded. Both source archive hashes match the registry checksums.

| Crate | Exact pin | Published | Publisher/owner | License | Registry checksum | Crate downloads (90 d, all versions) |
|---|---|---|---|---|---|---:|
| `sherpa-onnx` | `=1.13.8` | 2026-09-11 | `csukuangfj` / Fangjun Kuang (one owner) | Apache-2.0 | `efaa98d5f08380f3d47026e00f60fed182f6edaf4c0af290d88ac0e08d6e3b90` | 603,638 |
| `sherpa-onnx-sys` | `=1.13.8` | 2026-09-11 | `csukuangfj` / Fangjun Kuang (one owner) | Apache-2.0 | `14bbeabb73f73f1c1f4a278af0ee9ae90a19f9727482027047fb9422fa14f8fe` | 558,487 |

The wrapper's normal dependencies are `serde`, `serde_json`, and the exact `sherpa-onnx-sys =1.13.8` package. The sys crate has build dependencies `bzip2`, `tar`, `ureq` (proxy-from-env enabled), and `zip`. The OSV version queries for both exact crate versions returned no advisories. `cargo-audit` is not installed, so a separate RustSec database check and a full resolved transitive graph audit remain outstanding.

**Build-script finding:** the source was inspected at `/Users/admin/.local/state/scribular-parity/spike-02/crate-source/sherpa-onnx-sys-1.13.8/build.rs`. With neither `SHERPA_ONNX_LIB_DIR` nor a cached archive, it downloads a platform-specific static archive from a versioned GitHub release into `OUT_DIR` and extracts it. The script does not verify a pinned SHA-256 for that archive. This is a blocking supply-chain issue for integrating the default build as-is. A post-approval spike must first fetch the release asset through a controlled process, record and verify its SHA-256, and supply the extracted libraries via `SHERPA_ONNX_LIB_DIR` (or patch/fork the build to enforce the pin). Static linking is default; shared mode has explicit rpath and runtime-copy behavior that must be measured in the actual Tauri bundle. There is no `Command::new` or external shell invocation in the inspected build script. Do not allow Cargo's build script to fetch an unverified binary as a workaround.

## Model artifact

The existing Electron model was read locally without copying or modifying it:

- File: `3dspeaker_speech_campplus_sv_zh_en_16k-common_advanced.onnx`
- Size: 28,281,164 bytes
- SHA-256: `aa3cfc16963a10586a9393f5035d6d6b57e98d358b347f80c2a30bf4f00ceba2`
- Local source: `/Volumes/My Shared Files/Scribular/notes-assistant-electron/src/service/runners/diarization-models/3dspeaker_speech_campplus_sv_zh_en_16k-common_advanced.onnx`
- License and download origin: the official sherpa-onnx speaker-identification page points to the `speaker-recongition-models` release tag and says to check each selected model license. The release asset was downloaded into the outside-repo scratch directory and measured at exactly 28,281,164 bytes with SHA-256 `aa3cfc16963a10586a9393f5035d6d6b57e98d358b347f80c2a30bf4f00ceba2`, matching the Electron copy byte-for-byte by size and checksum. A public model mirror records Apache-2.0 and the same SHA-256; the upstream ModelScope page was not text-readable, so license should be verified directly from the upstream model card before manifest work.

Do not add this file to Tauri or grow retained audio during this milestone.

## Spike evidence (actual observations only)

Environment was inspected on 2026-11-04. The Electron ONNX model exists and its byte length and SHA-256 are recorded above. `notes-assistant-electron/node_modules/sherpa-onnx-node` and its native `.node`/ONNX Runtime libraries are absent. No fixed-clip embedding could be produced from the Electron runtime, and no Rust/native implementation was installed or built. **Measured cosine parity: not run.** No inference time, RSS, signed-build result, runtime size, or embedding parity is claimed.

The scratch evidence directory is `/Users/admin/.local/state/scribular-parity/spike-02`. It contains the two verified `.crate` source archives and their extracted source for audit, but no downloaded native library or fabricated output. A useful parity spike still needs the existing Electron runtime available, fixed clips on disk, the approved Rust/native candidate, and a script to capture both embedding vectors and cosine similarity. Required pass remains cosine similarity >= 0.99 on every fixed clip, followed by a Tauri/macOS packaged-runtime load check.

## Approval needed

Approve one path for the next phase:

1. **Preferred:** permit a throwaway post-approval spike using exact pins `sherpa-onnx =1.13.8` and `sherpa-onnx-sys =1.13.8`, after the remaining metadata/build-script/security review and with all fetched native artifacts isolated and hash-pinned.
2. **Alternative:** permit a throwaway source-built C API FFI spike with pinned and hashed sherpa-onnx/ONNX Runtime sources and artifacts.
3. **Decline:** stop spec 02 before any dependency/native runtime is introduced.

Approval of the spike is not approval to integrate the dependency or ship the feature. Integration requires the measured parity and packaging/signing evidence and a separate decision if the recommended path fails.

## References checked 2026-11-04

- [Official sherpa-onnx Rust API docs](https://docs.rs/sherpa-onnx/1.13.8/sherpa_onnx/): API and build behavior; latest docs currently show 2.0.1.
- [Official v1.13.8 speaker embedding example](https://raw.githubusercontent.com/k2-fsa/sherpa-onnx/v1.13.8/rust-api-examples/examples/speaker_embedding_extractor.rs): creates extractor, feeds a waveform, checks readiness and computes an embedding.
- [Official v1.13.8 example manifest](https://raw.githubusercontent.com/k2-fsa/sherpa-onnx/v1.13.8/rust-api-examples/Cargo.toml): exact example dependency version and static/shared features.
- [crates.io package metadata for `sherpa-onnx`](https://crates.io/crates/sherpa-onnx/1.13.8) and [`sherpa-onnx-sys`](https://crates.io/crates/sherpa-onnx-sys/1.13.8): publisher, owners, downloads, license, registry checksums, and dependencies checked with the crates.io API on 2026-11-04.
- [OSV query](https://osv.dev/list?ecosystem=crates.io&q=sherpa-onnx): exact-version queries for both crates returned no advisories; this is not a transitive dependency audit.
- [Official v1.13.8 release](https://github.com/k2-fsa/sherpa-onnx/releases/tag/v1.13.8): release date and upstream provenance.
- [Official v2.0.1 release](https://github.com/k2-fsa/sherpa-onnx/releases/tag/v2.0.1): recent release date; outside the cooldown window.
- [Official sherpa-onnx C speaker embedding API](https://github.com/k2-fsa/sherpa-onnx/blob/master/sherpa-onnx/c-api/docs/speaker-embedding.dox): C API capability and ownership pattern.
- [Official sherpa-onnx speaker identification page](https://k2-fsa.github.io/sherpa/onnx/speaker-identification/apk.html): release tag and reminder to verify the selected model's license.
- [3D-Speaker model card on ModelScope](https://modelscope.cn/models/iic/speech_campplus_sv_zh_en_16k-common_advanced): upstream model listing; page was not text-readable in this review.
- [Model mirror SHA-256/license listing](https://huggingface.co/bitsydarel/campplus-onnx/blob/main/3dspeaker_speech_campplus_sv_zh_en_16k-common_advanced.onnx): same SHA-256 as the upstream release artifact; mirror is secondary license evidence only.
- [sherpa-rs repository](https://github.com/thewh1teagle/sherpa-rs): repository currently recommends official Rust bindings.
- [ort 2.0.0-rc.15 docs](https://docs.rs/crate/ort/2.0.0-rc.15): release candidate date and package information.
