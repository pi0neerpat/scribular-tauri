# Dependency review: multi-speaker diarization

**Review date:** 2026-11-04
**Status:** Patrick approved the bounded official `sherpa-onnx`/`sherpa-onnx-sys =1.13.8` scratch spike on 2026-11-04. The spike remains **blocked before any native fetch/build** because RustSec/full-graph audit, native archive authentication, native license/CVE review, and resolved redirect/extraction behavior are not established. No crate, native runtime, or app integration has been added.
**Scope:** Spec 02 first milestone only. This review does not authorize feature integration.
**Policy:** Exact pins; newest stable release at least two weeks old; owners, recent downloads, license, advisories, and build scripts reviewed before adding a dependency. Version and provenance facts must be rechecked when the gate clears.

## Approval proposal

The only candidate proposed for an isolated spike is the official `sherpa-onnx` Rust API at `=1.13.8` with its exact `sherpa-onnx-sys =1.13.8` dependency. The prior live research found v1.13.8 published 2026-09-11; the v2.0.0 and v2.0.1 releases were within the project's two-week cooldown on 2026-11-04. The local source archive contains the speaker embedding API. The official wrapper's default feature selects static linking through the sys crate.

Patrick's approval authorizes only the bounded scratch spike, subject to the technical gates below. It does not authorize adding either crate to this app, shipping native code, or changing audio retention. No native archive has been obtained or measured. Native archive size and any app bundle-size delta are therefore **not measured**; this review makes no pre-approval or post-approval bundle-size claim. Measure them only if the native gates clear and the spike proceeds.

### Candidate crate record

The prior read-only crates.io API responses reported the following. Source archive SHA-256 values matched the checksum advertised in those responses; this establishes that those local source archives match the registry-advertised bytes, not who authored them or whether the build is trustworthy.

| Crate | Proposed exact pin | Published | Registry publisher / owners | License metadata | Registry checksum | 90-day downloads, all versions |
|---|---|---|---|---|---|---:|
| `sherpa-onnx` | `=1.13.8` | 2026-09-11 | `csukuangfj` / Fangjun Kuang; one owner | Apache-2.0 | `efaa98d5f08380f3d47026e00f60fed182f6edaf4c0af290d88ac0e08d6e3b90` | 603,638 |
| `sherpa-onnx-sys` | `=1.13.8` | 2026-09-11 | `csukuangfj` / Fangjun Kuang; one owner | Apache-2.0 | `14bbeabb73f73f1c1f4a278af0ee9ae90a19f9727482027047fb9422fa14f8fe` | 558,487 |

Both crates have a single registry owner/publisher. The prior registry response had no `trustpub_data`; an independently reproducible crates.io publisher-to-source provenance chain was not established here. No source-to-source comparison between the crates.io 1.13.8 package and the upstream v1.13.8 Git tag was completed. The prior version snapshot showed 1.13.8 as the latest eligible stable 1.x pin, but this review does not establish that no newer 1.x exists now or that any newer 1.x has trustworthy provenance. Recheck crate versions, owner count, publishing provenance, and checksums before any spike because these facts can change. The public repository/tag and registry package do not alone authenticate a native release archive.

The wrapper's normal dependencies are `serde`, `serde_json`, and exact `sherpa-onnx-sys =1.13.8`. The sys crate's build dependencies are `bzip2`, `tar`, `ureq` (with `proxy-from-env`), and `zip`. The packaged source archive includes a `Cargo.lock` resolving `ureq` to 2.12.1, but that is not the application's lockfile and does not pin the consumer's resolution. A candidate app lockfile and its full transitive graph do not exist.

## Security audit status and owner

The previous session queried OSV for the two exact crate versions and received empty advisory lists. That was a direct-package query only; it did not audit transitive Rust dependencies, native artifacts, or the ONNX Runtime code. In this session, `command -v cargo cargo-audit cargo-deny cargo-vet osv-scanner sandbox-exec` found Cargo and macOS `sandbox-exec`; it found none of the four audit tools. `/Users/admin/.cargo/registry` exists, but no local `advisory-db` directory or cached `ureq-*`, `tar-*`, or `zip-*` source trees were found. No Internet or registry request was made for this gate review; the only socket attempt was the local sandbox-denial probe documented below. The only observed lockfile is packaged in the sys crate source; it resolves that package's source workspace and is not the consumer candidate lockfile. No isolated candidate graph has been generated or audited.

**Unresolved gate; owner: Codex (implementation owner).** Obtain an approved RustSec audit tool plus current advisory database, resolve the exact candidate graph in an isolated scratch manifest without running its build script, and record the complete transitive audit. The available direct-package OSV result and packaged source lock are insufficient. Until this is done, do not fetch or build a native archive. If approved audit tooling/data cannot be made available, report that exact blocker to Patrick for a wait/decline decision. No clean transitive audit is claimed.

The candidate dependency must not be added to the app manifest merely to create a graph. A scratch manifest may be created under the approved spike boundary to resolve metadata/lockfile only; it must not run the sys build script or native fetch until all gates in this review are cleared.

## Exact build-script behavior observed in local source

The audited files are the prior session's downloaded `.crate` source archives and extracted source under `/Users/admin/.local/state/scribular-parity/spike-02/crate-source/`. They were not compiled. Findings below describe source inspection, not an executed build.

- `sherpa-onnx` 1.13.8 has no `build.rs` in the packaged source. Its default feature is `static`, which enables `sherpa-onnx-sys/static`. The sys crate owns the native fetch/link build script.
- The sys script selects static mode by default on macOS; the `shared` feature selects shared mode. Setting both features errors. The script supports macOS `x86_64` and `aarch64` for both modes. The app's actual release target matrix was not rechecked in this docs-only revision.
- `SHERPA_ONNX_LIB_DIR`, when set to an existing directory, is checked first and returned directly. That bypasses both the archive cache and the HTTP branch; the script only checks that the path is a directory before emitting link directives.
- The wrapper crate has no `build.rs`; its default `static` feature enables the sys crate's `static` feature, which is why the sys build script runs as part of a consumer build. `DOCS_RS` skips native setup/linking, but is not a build isolation mechanism. No shell command invocation was found in the sys build script. A supplied `SHERPA_ONNX_LIB_DIR` must reach the sys build-script environment; it bypasses this script's download/cache resolution only and does not constrain networking by other build scripts.
- Without that variable, the script uses a target-dir cache named `sherpa-onnx-prebuilt`. It first accepts an existing extracted `lib/` directory without rehashing it. Otherwise, if the expected archive is absent and `SHERPA_ONNX_ARCHIVE_DIR` is set, it copies the expected local archive into the cache or errors if that file is missing. If the archive is absent and `SHERPA_ONNX_ARCHIVE_DIR` is unset, it calls `ureq` on the release URL.
- The inspected source constructs versioned URLs under `https://github.com/k2-fsa/sherpa-onnx/releases/download/v1.13.8/` for normal platform archives; iOS uses a separate `xcframework` release path. Mac archive names are `sherpa-onnx-v1.13.8-osx-x64-{static-lib|shared-lib}.tar.bz2` and `sherpa-onnx-v1.13.8-osx-arm64-{static-lib|shared-lib}.tar.bz2`. These names/URLs are source-derived; no native archive was fetched in this review.
- The script's `ureq` builder calls `.try_proxy_from_env(true)`, so proxy environment configuration is consulted. The packaged sys `Cargo.toml` allows `ureq ^2.12`; 2.12.1 appears in the source archive's own lockfile only. The build script specifies no redirect policy or host allowlist. The actual redirect behavior of the consumer-resolved `ureq` and the effect of proxy configuration on redirect destinations were not inspected because that resolved source is not locally cached; redirects remain an explicit gate, and destinations must not be assumed constrained.
- Download and local-copy writes use a sibling `.part` file followed by rename; the downloaded file is `sync_all`'d before rename. The script does not verify a pinned digest or signature before extraction. A `.tar.bz2` is unpacked into the cache root via `tar::Archive::unpack`; an iOS zip is extracted under its archive directory via `zip::ZipArchive::extract`. On unpack failure it attempts to remove the archive and extracted directory. The resolved `tar` and `zip` source versions were not available locally, so path traversal, symlink, permission, and overwrite behavior of those implementations remains unreviewed. Existing extracted cache directories are accepted without archive digest/content verification. `SHERPA_ONNX_LIB_DIR` only checks `is_dir()` and returns the directory; it does not validate library contents. `SHERPA_ONNX_ARCHIVE_DIR` selects a named local archive but the script copies and extracts it without authentication.
- In shared mode the script adds rpath link args and copies runtime dylibs into Cargo profile output and its `examples/` directory. For Android/iOS targets it also has code that copies libraries/frameworks into Tauri-generated/project directories. These paths were read in source, not executed. The macOS app bundle/signing path was not tested.
- No `Command::new`/external shell invocation was found in the inspected sys build script.

### Required no-uncontrolled-network procedure for the approved spike

Patrick has approved the bounded spike. The procedure below is still **not executed** and does not clear the outstanding audit/provenance/licensing/redirect gates:

1. Identify the exact platform archive and establish a trustworthy provenance source before accepting it. Prefer an upstream signature/attestation or a checksum delivered through an independently trusted channel. A SHA-256 computed after downloading an unauthenticated archive only fingerprints those received bytes; it does **not** authenticate their origin. If no independent authentication can be established, stop and return for a new dependency decision.
2. Fetch only the approved asset in an isolated preparation step, record its source URL, platform target, size, authentication evidence, and SHA-256, then audit/extract it outside Cargo. Do not put it in the app bundle or source tree during the spike.
3. Prepare the exact audited Rust dependency graph in an offline Cargo cache. Build only with `SHERPA_ONNX_LIB_DIR` pointing at the reviewed extracted library directory and confirm the directory exists before invoking Cargo. Set `CARGO_NET_OFFLINE=true`; additionally invoke Cargo under macOS `sandbox-exec` with a deny-by-default profile that allows required process execution, reads, and writes only to the scratch target/cache paths, while denying `network*`. Cargo offline mode alone does not prevent arbitrary build scripts from opening sockets. The valid `SHERPA_ONNX_LIB_DIR` is the source-observed bypass for this crate's archive-fetch path, but it does not prevent other build scripts from networking. This session verified the OS-level primitive with `sandbox-exec -p '(version 1)(deny default)(allow process*)(allow file-read*)(allow file-write*)(deny network*)' /usr/bin/python3 -c 'import socket; s=socket.socket(); s.settimeout(1); s.connect(("192.0.2.1", 9))'`; it exited 1 with `PermissionError: [Errno 1] Operation not permitted`. This was a denied-socket probe only, not a Cargo build or validation of a scratch-path write allowlist. The exact build profile must be tested after the other gates clear and before any build.
4. Record the resulting target, exact artifact digests, bundle-size delta, and runtime loading result. Do not treat a successful link as evidence of signing/notarization or parity.

The OS network-denial primitive is demonstrated by the probe above. A correctly scoped scratch-build profile and offline Cargo build have not been tested. No native build was run. The resolved `ureq` redirect behavior remains unaudited. These are gates, not assumed protections.

## Native archive and licensing limits

No sherpa-onnx native archive, ONNX Runtime library, or platform binary is present in the scratch evidence. The versioned release URL pattern and archive-name construction were read from the sys build script; this is not evidence that a particular release asset exists, nor does it establish the asset's uploader, provenance, signature/attestation, independent checksum, SBOM, contents, or supported target matrix. No asset metadata was retrieved in this review. A SHA-256 computed after download would identify those received bytes for repeatability only; it cannot authenticate the origin or integrity of an unauthenticated download. Trusted native-archive provenance is therefore unresolved, and no archive may be fetched until an independent authentication path for the exact target asset is documented.

The crates' Apache-2.0 metadata does not establish licenses for the bundled native libraries, ONNX Runtime, or their transitive native components. No native license inventory, native CVE/advisory review, macOS architecture verification, archive size, or code-signing/notarization result exists. **These remain mandatory pre-fetch gates owned by Codex**, including a license/SBOM inventory for the exact archive and a CVE review against the component versions it actually contains. The registry has one owner/publisher and no `trustpub_data` was present in the previously observed response; that provenance limitation also remains for the crate packages. The prior “newest eligible 1.x” snapshot does not establish current newer-1.x availability or provenance; recheck and document the 1.13.8 choice against current registry evidence before proceeding.

## Model artifact: equality evidence, not provenance authentication

The earlier session fetched the model release asset to the existing scratch directory (no fetch was performed during this revision). It recorded 28,281,164 bytes and SHA-256 `aa3cfc16963a10586a9393f5035d6d6b57e98d358b347f80c2a30bf4f00ceba2`; the existing Electron model had the same size and digest. This supports byte equality between those two local files. The digest was computed after download and does not authenticate the release asset or its origin. The inspected public mirror states Apache-2.0 and the same digest, but is secondary evidence; the upstream ModelScope page was not text-readable in the prior review. Verify the upstream model-card license and provenance before any model-manifest integration; this is a required gate, not a verified license conclusion. No audio-retention boundary changes are proposed; do not add or retain audio as part of this milestone.

## Electron baseline and clip availability

The comparison baseline for this review is the Electron app v1.5.10 named by spec 02. Its `package.json` pins `sherpa-onnx-node` 1.12.31; the prior local `yarn.lock` inspection showed optional platform packages resolving to 1.12.36. The Electron `node_modules/sherpa-onnx-node` package and native `.node`/ONNX Runtime libraries were absent. Therefore only the declared/locked baseline is known; the actual installed Electron native version could not be observed or used.

No fixed WAV clip was found in the Electron checkout's test tree during the prior local file scan. Its diarization E2E helper starts and stops a live microphone recording, so that path is not a reproducible embedding baseline. This Tauri checkout contains `src-tauri/tests/fixtures/audio/2-speakers-12s.wav`; it is one available fixture, not an established shared parity set, and its licensing/provenance and suitability for embedding-level parity were not established here. No fixed clips were run through either engine, and no embedding vectors or cosine scores exist. A post-approval spike must define a fixed, approved clip set and document sample rate, segment boundaries, and clip provenance before comparing engines.

## Actual observations and unmeasured results

- The existing Electron model and the prior scratch copy have matching locally measured size and SHA-256; this is an equality observation only.
- The two source `.crate` archives in scratch matched their registry-advertised checksums in the previous session; this is not provenance authentication.
- The Electron native runtime is absent; the proposed Rust/native engine was not built.
- **Measured embedding parity: not run.** No fixed clip set or baseline embedding vectors are available.
- Native archive size, app bundle-size change, inference latency, peak RSS, dylib loading, signed/notarized runtime behavior, native CVEs/licenses, and end-to-end performance are **unmeasured/unreviewed**.
- Existing automated test logs under `spike-02/checks/` predate this docs revision and cover the repository, not the proposed dependency or parity behavior.

## Scratch evidence directory contents

At the time of this revision, `/Users/admin/.local/state/scribular-parity/spike-02/` contains:

- the previously downloaded 28,281,164-byte ONNX model artifact;
- the `sherpa-onnx` and `sherpa-onnx-sys` 1.13.8 source `.crate` archives and extracted source trees, used for inspection only;
- earlier repository check logs under `checks/`; and
- this separate scratch `README.md`.

It contains no native sherpa/ONNX Runtime archive, no compiled or installed candidate crate, no parity script, and no measured embedding output. Cargo source archives and extracted source trees are present for read-only inspection. The model and audio are not to be copied into retained app data or committed as part of this first milestone.

## Decision and exact next action

Patrick has approved the **bounded official `sherpa-onnx`/`sherpa-onnx-sys =1.13.8` scratch spike**, but it is not yet safe to execute. **No native fetch/build is authorized by this checkpoint** until Codex records a full RustSec transitive audit, independent authentication for the exact native asset, native component license/CVE inventory, resolved `ureq` redirect and `tar`/`zip` extraction behavior, and a demonstrated network-denied build procedure. The model-card license/provenance check and revalidation of the one-owner publisher and newer-1.x/provenance snapshot are also required before dependency integration. Current blocker owner is Codex for gathering these technical evidence/tool prerequisites; if an independently authenticated native asset or approved audit source cannot be established, Patrick owns the wait/decline dependency decision. Do not switch engines or expand the spike.

A source-built C API FFI is **not an approved fallback** and is not covered by this spike. The prior work only confirmed that sherpa-onnx exposes a C speaker-embedding API. There is no selected source-build commit/tag, recursive submodule/dependency lock, fetch URL/provenance record, ONNX Runtime acquisition/authentication plan, target/compiler/link flags, build-script/network audit, license/SBOM inventory, CVE review, or reproducibility evidence. A separate dependency review and explicit approval are required before fetching source or native artifacts for this route.

Resume only after Codex has recorded that every pre-fetch gate above is clear. The exact CWF resume transition is:

```sh
cwf transition implementing --reason "Pre-fetch gates cleared with evidence in plans/DEPENDENCY_REVIEW_DIARIZATION.md: full RustSec transitive audit, independent authentication for the exact native asset, native license/CVE inventory, resolved redirect/extraction behavior, and demonstrated network-denied build procedure; proceed only with Patrick's bounded sherpa-onnx 1.13.8 scratch-spike approval"
```

This approval is not approval to integrate or ship the feature. The spec 02 audio-retention boundary remains unchanged.

## References and local evidence

- Official Rust API docs and v1.13.8 speaker embedding example referenced in the prior review: [API docs](https://docs.rs/sherpa-onnx/1.13.8/sherpa_onnx/), [example source](https://raw.githubusercontent.com/k2-fsa/sherpa-onnx/v1.13.8/rust-api-examples/examples/speaker_embedding_extractor.rs), [example manifest](https://raw.githubusercontent.com/k2-fsa/sherpa-onnx/v1.13.8/rust-api-examples/Cargo.toml).
- Prior registry API snapshot: [sherpa-onnx 1.13.8](https://crates.io/crates/sherpa-onnx/1.13.8) and [sherpa-onnx-sys 1.13.8](https://crates.io/crates/sherpa-onnx-sys/1.13.8). No registry query was made for this revision.
- Local source inspected in `/Users/admin/.local/state/scribular-parity/spike-02/crate-source/sherpa-onnx-sys-1.13.8/build.rs`, `Cargo.toml`, and `Cargo.lock`; source archive checksums are recorded above. No native release asset was authenticated.
- Electron baseline inspected locally at `/Volumes/My Shared Files/Scribular/notes-assistant-electron/package.json`, `yarn.lock`, and `test/helpers/diarization.ts`; Electron version is v1.5.10, but the native package is not installed.
