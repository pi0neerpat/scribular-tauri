---
title: llama.cpp dylibs load under cargo but abort when the binary runs on its own
date: 2026-10-20
category: runtime-errors
module: src-tauri build and linking (build.rs, llama-cpp-2 dynamic-link)
problem_type: runtime_error
component: tooling
symptoms:
  - "`target/release/bench` exits 134 with `dyld: Library not loaded: @rpath/libggml-base.0.dylib`"
  - "`cargo test` and `cargo run` pass, so the failure only shows when the binary is started directly"
root_cause: incomplete_setup
resolution_type: config_change
severity: medium
related_components:
  - infrastructure
tags: [rpath, dylib, dyld, llama-cpp, ggml, dynamic-link, build-rs, tauri-bundle]
---

# llama.cpp dylibs load under cargo but abort when the binary runs on its own

## Problem

`llama-cpp-2` uses the `dynamic-link` feature (`src-tauri/Cargo.toml:33`), so
llama.cpp and its ggml are built as dylibs that the binary loads through
`@rpath`. When `build.rs` only added the app-bundle rpath, every binary worked
under cargo and aborted when started directly. The packaged `.app` breaks the
same way unless the dylibs are bundled.

## Symptoms

- `target/release/bench ...` exits 134:
  `dyld: Library not loaded: @rpath/libggml-base.0.dylib`. dyld's "tried:"
  list names only `<binary dir>/../Frameworks/...`.
- `cargo test`, the ignored real-model tests and `cargo run --bin bench` all
  pass, because cargo sets the search path for you (see below).

## What Didn't Work

- **Testing only through cargo.** Every test binary and `cargo run` passed
  while the rpath was wrong. The U5 bench was the first binary anyone ran
  directly.
- **The U1 link spike (session history).** In the session that chose fallback
  A, the spike binary had no `LC_RPATH` at all. It ran with
  `DYLD_LIBRARY_PATH` pointed at the target dir, which hides the same gap
  (`plans/DEPENDENCY_REVIEW_ENGINES.md:211-214`).

## Solution

`src-tauri/build.rs:6-7` adds two rpaths, for macOS targets only:

```rust
println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../Frameworks"); // .app bundle
println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path");              // target/<profile>
```

Cargo copies the six dylibs (`libllama`, `libllama-common`, `libggml`,
`libggml-base`, `libggml-cpu`, `libggml-metal`) into both `target/<profile>/`
and `target/<profile>/deps/`. So `@executable_path` resolves for the bench
binary and for the test binaries alike. Check the result with:

```sh
otool -l target/release/bench | grep -A2 LC_RPATH   # expect both paths
```

**Still open:** the `.app` only has the `../Frameworks` rpath to search.
`tauri.conf.json` has no `bundle.macOS.frameworks` entry yet, so a packaged
build will abort with this same dyld error until the dylibs are listed there
and signed with the app.

## Why This Works

On macOS, `@rpath/...` install names are resolved against the binary's
`LC_RPATH` entries. `cargo run` and `cargo test` also set
`DYLD_FALLBACK_LIBRARY_PATH` to include `target/<profile>` and
`target/<profile>/deps`; this was checked on this machine with a binary that
prints the variable. dyld falls back to that path after the rpaths fail, so
under cargo a missing rpath never shows. A binary started from a shell, or by
Finder, has neither, and aborts before `main`.

Repro without editing `build.rs`: copy the binary away from its dylibs. Both
lines were run on this machine with the fixed `build.rs`. In the second one,
`bench` exits 1 only because it was given no arguments.

```sh
mkdir -p /tmp/x && cp target/release/bench /tmp/x/ && /tmp/x/bench   # exit 134, the dyld error
DYLD_FALLBACK_LIBRARY_PATH=$PWD/target/release /tmp/x/bench   # loads
```

## Prevention

- Run any new binary target, and the built `.app`, **directly** at least once.
  A green `cargo test` says nothing about rpaths.
- After changing link features or `build.rs`, check `otool -l <bin> | grep -A2
  LC_RPATH` and `otool -L <bin>` for `@rpath` entries.
- Before shipping the app, list the dylibs in `bundle.macOS.frameworks`. Then
  launch `Contents/MacOS/<app>` from a terminal so a dyld abort shows on
  stderr rather than as a silent crash.
- Keep the usage rule from fallback A, which `src-tauri/clippy.toml` already
  enforces. Once whisper-rs lands, ggml is present both statically (whisper)
  and as a dylib (llama), and calls from the main binary bind to whisper's
  copy.

## Related Issues

- `plans/DEPENDENCY_REVIEW_ENGINES.md`: why `dynamic-link` (fallback A), the
  rpath and bundling notes, and the follow-up at the end of the file.
- `docs/solutions/workflow-issues/lockfile-publish-date-cutoff-check.md`: same
  dependency area, but a different problem.
