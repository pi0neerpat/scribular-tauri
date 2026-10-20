# Dependency review: engines (whisper-rs, llama-cpp-2)

**Date:** 2026-10-19
**Status:** Approved 2026-10-19 (fallback A pre-approved). The ggml link check (U1) runs
on these pins, and its result is recorded below.
**Policy:** same as `DEPENDENCY_REVIEW_M0.md` (review, audit, exact pin, in-house
check, read `build.rs` like an npm postinstall).

**Version rule used:** the newest stable release that is at least 2 weeks old
(published on or before 2026-10-05).

These crates are reviewed in U1, not U3/U4. The link check needs both crates at their
final pins, because the risk being tested (two copies of ggml in one binary) depends on
the exact vendored ggml versions.

---

## Rust crates

| Crate | Pin | Published | Recent DL (90 d) | Owners | License | OSV / RustSec |
|---|---|---|---|---|---|---|
| `whisper-rs` | `=0.16.0` | 2026-03-12 | 0.91 M (1.66 M total) | **tazz4843 (1)** | Unlicense | clean |
| `whisper-rs-sys` | `=0.15.0` | 2026-03-12 | (same project) | **tazz4843 (1)** | Unlicense | clean |
| `llama-cpp-2` | `=0.1.158` | 2026-09-30 | 0.80 M | **MarcusDunn (1)** | MIT OR Apache-2.0 | clean |
| `llama-cpp-sys-2` | `=0.1.158` | 2026-09-30 | (same project) | **MarcusDunn (1)** | MIT OR Apache-2.0 | clean |

The downloaded `.crate` files match the registry sha256 checksums (whisper-rs
`2088172d…29f6`, whisper-rs-sys `6986c0fe…a56f`, llama-cpp-2 `5a89d1d5…cbe`,
llama-cpp-sys-2 `608edbee…bec`).

**Notes**
- **Newer llama-cpp-2 releases are skipped on purpose.** `0.1.159` (2026-10-07) and
  `0.1.160` (2026-10-15) are too new under the two-week rule. The project releases about
  once a week, tracking upstream llama.cpp.
- **Single owner on both.** Each crate has one crates.io owner. whisper-rs lives on
  Codeberg (`codeberg.org/tazz4843/whisper-rs`); llama-cpp-2 is in the `utilityai`
  GitHub org. The real code is the vendored C++ (whisper.cpp, llama.cpp) from ggml-org.
  The Rust layers are thin bindings, so a fork is practical if either crate stalls.
- **Features to use:**
  - `whisper-rs = { version = "=0.16.0", features = ["metal"] }`. Default features are
    empty. `metal` turns on `whisper-rs-sys/metal`.
  - `llama-cpp-2 = { version = "=0.1.158", default-features = false, features = ["metal"] }`.
    The defaults (`openmp`, `android-shared-stdcxx`, `common`) aren't needed on macOS.
    Turning off `openmp` avoids needing libomp, which Xcode clang doesn't ship.
- **MSRV:** whisper-rs-sys declares `rust-version = 1.88.0`. Current stable meets it.

### Vendored sources

| Crate | Vendors | ggml version |
|---|---|---|
| `whisper-rs-sys 0.15.0` | whisper.cpp 1.8.3 | **0.9.5** |
| `llama-cpp-sys-2 0.1.158` | llama.cpp (Sept 2026) | **0.24.0** |

### build.rs review

**whisper-rs-sys** (395 lines):
- No network access. No downloads.
- Copies `./whisper.cpp` into `OUT_DIR` and builds it there with CMake, as `Release` with
  `BUILD_SHARED_LIBS=OFF`.
- Runs bindgen. If bindgen fails, it uses the bundled bindings.
- With `metal`, it sets `GGML_METAL=ON` and `GGML_METAL_EMBED_LIBRARY=ON` (the shader is
  compiled into the binary, so there's no `.metallib` to ship).
- Passes any env var starting with `WHISPER_`, `GGML_` or `CMAKE_` through to CMake.
  This matters, because a `GGML_*` var set for one crate also reaches the other.
- Statically links `whisper`, `ggml`, `ggml-base`, `ggml-cpu`, `ggml-blas` (macOS) and
  `ggml-metal`, plus the frameworks Accelerate, Foundation, Metal and MetalKit, and `c++`.
- One oddity: it deletes `bindings/javascript/package.json` relative to the crate
  source directory. It's harmless, but it is a write outside `OUT_DIR`.

**llama-cpp-sys-2** (1,404 lines):
- No network access. `LLAMA_CURL=OFF`. Tests, examples, server, tools and the unified
  `llama` app are all turned off.
- Its one `Command::new` runs the C compiler with `--print-file-name=<archive>` to find
  a static runtime library. That's a read-only probe.
- Reads only build-config env vars: `LLAMA_LIB_PROFILE`, `LLAMA_BUILD_SHARED_LIBS`,
  `LLAMA_STATIC_CRT`, `CMAKE_*` and `GGML_*` (passed to CMake), and `BUILD_DEBUG`.
- On Apple it sets `GGML_BLAS=OFF`, unlike whisper-rs-sys, which turns BLAS on.
- Links `static` by default. With `dynamic-link`, or when `LLAMA_BUILD_SHARED_LIBS=1`,
  it links `dylib`.
- With `system-ggml`, it sets `LLAMA_USE_SYSTEM_GGML=ON` and links an external
  `ggml`/`ggml-base`/`ggml-cpu`. It also exports `cargo:ggml_cmake_dir`, so a dependent
  crate can build against llama's ggml. A comment there says "Two copies of ggml in one
  program cause duplicate symbols." Upstream already knows about this clash.
- macOS links: Foundation, Metal, MetalKit, Accelerate, `c++`.

Both scripts are long but do what they say: build vendored C++ with CMake and emit link
flags. Nothing reaches outside the build tree except the whisper `package.json`
deletion noted above.

**Toolchain:** Xcode 26.1.1 (17B100) and CMake are installed. bindgen needs libclang,
which comes with Xcode.

---

## ⚠️ Main risk: two ggml copies

Both crates statically link their own ggml, and the versions are far apart (0.9.5 vs
0.24.0). Both export the same C symbol names (`ggml_init`, `ggml_backend_*`, …). There
are two ways this can go wrong:

1. **Duplicate-symbol link error.** If the linker pulls the same-named object files from
   both archives, it fails loudly. This is the easy case.
2. **Silent cross-binding (worse).** ld64 resolves each undefined symbol from the first
   archive that defines it and doesn't look further. So whisper's calls can bind to
   llama's ggml 0.24 (or the other way round). The link succeeds, and you get a crash or
   corrupted output at runtime because the struct layouts differ.

So "it links" doesn't pass the check. The spike has to **run** one real call into each
engine that goes through ggml, in the same process.

### Fallbacks, in order of preference

| # | Fallback | How | Assessment |
|---|---|---|---|
| A | **llama as a dylib** | `llama-cpp-2` feature `dynamic-link`: llama and its ggml build as `.dylib`s; whisper stays static | **Best bet.** macOS uses a two-level namespace, so references from `libllama.dylib` are bound to `libggml.dylib` by name, and the static copy in the main binary can't take them over. Cost: bundle 4–5 dylibs in the `.app` and set `@rpath`. Tauri supports this through `bundle.macOS.frameworks`. |
| B | **One shared ggml** | `system-ggml` / `system-ggml-static` on llama, with whisper building against the same ggml | **Unlikely to work.** whisper.cpp 1.8.3 targets ggml 0.9.5. Building it against 0.24.0 would probably hit API breaks, and fixing them means patching vendored C++. |
| C | **whisper as a dylib** | Wrap whisper-rs in a small `cdylib` crate (or the sys crate built with `WHISPER_`/`CMAKE_` shared-lib vars) loaded by the app | Works for the same two-level-namespace reason. More plumbing than A, because whisper-rs has no dylib feature. |
| D | **Separate process** | Run STT in a sidecar binary | Always works, but adds IPC and a second signed binary. Out of scope for the prototype unless A–C fail. |

If none of A–C works, that is the plan's stop condition ("the ggml link check fails
with no working fallback").

### Spike design (`spikes/ggml-link/`, deleted after)

- A throwaway binary crate with both crates on `metal`, pinned exactly as above.
- Target dir outside the VirtioFS share, for speed.
- `main` does, in order:
  1. llama: `LlamaBackend::init()`, then load a tiny GGUF model and tokenize a string.
  2. whisper: load `ggml-tiny.en.bin`, then run `full()` on 1 s of silence.
  3. Print both libraries' system-info strings, which list the ggml backends each one
     actually registered.
- Pass means both calls return without crashing and each system-info shows Metal.
- `nm` on the binary to see which ggml symbols are defined and how many copies exist.
- The model files are test fixtures, not dependencies. They're downloaded into the
  scratchpad and not committed.

**Environment note:** this machine is a macOS 26.2 VM ("Apple M2 (Virtual)", 8 GB)
with paravirtualized Metal. If Metal fails to initialize here but CPU works, that's a
VM limitation, not a link failure. In that case I'll record it and re-check on real
hardware before U3.

---

## Link check result

**Run 2026-10-19 on this machine (macOS 26.2 VM, Xcode 26.1.1, rustc 1.99.1).**
**Verdict: static linking fails. Fallback A works, with one usage rule.**

### Pin gap found first

`llama-cpp-2 =0.1.158` depends on `llama-cpp-sys-2 = "^0.1.158"`, so Cargo resolved
the sys crate to **0.1.160** (2026-10-15), which breaks the two-week rule. The fix is
to also pin both sys crates directly in `Cargo.toml`:

```toml
whisper-rs-sys = "=0.15.0"
llama-cpp-sys-2 = "=0.1.158"
```

U3 and U4 must keep these direct pins. All results below are on the corrected pins.

### Static (both crates default, Metal)

- The build "succeeds", but the linker prints **353 `duplicate symbol` warnings**
  (`_ggml_validate_row_data`, `_ggml_backend_dev_host_buffer_type`, …) and keeps one
  copy of each.
- Run: llama loads and offloads to Metal, then **aborts** in its first `decode` with
  `GGML_ASSERT(a->op == GGML_OP_FLASH_ATTN_EXT) failed` (exit 134). This is the silent
  cross-binding case: llama's code ran against the wrong ggml.
- ❌ Not usable.

### Fallback A: `llama-cpp-2` feature `dynamic-link`

- Build: **0 duplicate-symbol warnings.** llama and ggml 0.24.0 are built as
  `libllama`, `libllama-common`, `libggml`, `libggml-base`, `libggml-cpu` and
  `libggml-metal` (`.0.dylib` each, linked via `@rpath`). whisper and its ggml 0.9.5 stay
  static in the main binary.
- Run (exit 0):
  - `LLAMA OK: 5 tokens, 32000 logits` on device `MTL0` (ggml 0.24.0 naming).
  - `WHISPER OK: 1 segments`, "using Metal backend" on device `Metal` (ggml 0.9.5
    naming), `EMBED_LIBRARY = 1`.
  - The different device names confirm each engine used its own ggml.
- `nm`: the main binary defines 577 `_ggml*` symbols (whisper's copy) and imports
  **none**. It imports 17 `_llama*` symbols from `libllama.0.dylib`. Inside the dylibs,
  two-level namespace keeps llama's ggml calls bound to `libggml*.dylib`.
- ✅ Usable, with the rule below.

### ⚠️ Usage rule under fallback A

A few **Rust** functions in `llama-cpp-2` call `ggml_*` directly. Those calls are made
from the main binary, so they bind to **whisper's** ggml 0.9.5, not llama's. U4 must not
use:

| Function | Calls | What would go wrong |
|---|---|---|
| `llama_cpp_2::send_logs_to_tracing` | `ggml_log_set` | Sets whisper's ggml log callback, not llama's. Mostly harmless, but misleading |
| `llama_cpp_2::list_llama_ggml_backend_devices` | `ggml_backend_dev_*` | Lists whisper's devices. Props struct layout may differ (UB) |
| `LlamaModelParams::with_devices` / `devices` | `ggml_backend_dev_*` | Passes device handles from the wrong ggml into llama (UB) |
| `LlamaModelParams::add_cpu_buft_override` | `ggml_backend_cpu_buffer_type` | Passes a buffer type from the wrong ggml (UB) |
| `LlamaBackend::load_backends_from_path` | `ggml_backend_load_all_from_path` | Loads into whisper's registry |
| `llama_cpp_2::ggml_time_us` | `ggml_time_us` | Harmless (a clock), but avoid it for consistency |

The spike's path (backend init, model load with `with_n_gpu_layers`, context, tokenize,
decode, logits) doesn't touch any of these. **Enforcement:** add these paths to
`disallowed-methods` in `src-tauri/clippy.toml` when U4 adds llama-cpp-2. Logging can
use `llama_cpp_2::llama_backend`'s void-log setting or llama's own `llama_log_set`
callback.

### Bundling notes for later units

- The spike binary has **no `LC_RPATH`**, so it ran with `DYLD_LIBRARY_PATH` set to the
  target dir. The app needs `-Wl,-rpath,@executable_path/../Frameworks` (via
  `build.rs` `cargo:rustc-link-arg`) and the six dylibs listed in
  `bundle.macOS.frameworks`, and they must be signed with the app.
- `cargo tauri dev` will need the rpath to point at the target dir (or `DYLD_LIBRARY_PATH`).
- Metal works in this VM (paravirtual device, `MTLGPUFamilyApple5`). Real-hardware
  performance is still U6's job.

The spike (`spikes/ggml-link/`) is deleted. The test models stay in the scratchpad,
uncommitted.

---

## Recommendation

| Group | Recommendation |
|---|---|
| `whisper-rs` `=0.16.0` + `whisper-rs-sys` `=0.15.0` (feature `metal`) | ✅ Approve, plus direct sys-crate pins (`whisper-rs-sys =0.15.0`, `llama-cpp-sys-2 =0.1.158`). Link check passed with fallback A |
| `llama-cpp-2` `=0.1.158` + `llama-cpp-sys-2` `=0.1.158` (no default features, `metal`) | ✅ Approve, plus direct sys-crate pins (`whisper-rs-sys =0.15.0`, `llama-cpp-sys-2 =0.1.158`). Link check passed with fallback A |
| Fallback A (`dynamic-link` feature on llama-cpp-2) | ✅ **Required.** The static link failed (see above). Same crate and pin, only a different feature |
| Test model files (tiny GGUF, `ggml-tiny.en.bin`) | Scratchpad only, not committed |

**In-house:** not feasible. These are the inference engines.
