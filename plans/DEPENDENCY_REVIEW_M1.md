# Dependency review: M1 (session store)

**Date:** 2026-10-19
**Status:** Approved 2026-10-19.
**Policy:** `../notes-assistant-electron/CLAUDE.md` § Dependency Policy (review, audit,
exact pin, in-house check).

**Version rule used:** the newest stable release published on or before 2026-10-05.

---

## Rust crates

| Crate | Pin | Published | Recent DL (90 d) | Owners | License | OSV / RustSec |
|---|---|---|---|---|---|---|
| `rusqlite` (feature `bundled`) | `=0.40.2` | 2026-08-08 | 45.7 M | thomcc, gwenn | MIT | clean |
| `libsqlite3-sys` | `=0.38.2` | 2026-08-08 | (rusqlite repo) | thomcc, gwenn | MIT | clean |

`libsqlite3-sys 0.38.2` bundles **SQLite 3.53.2**.

**Notes**
- This is KTD1 in the plan: SQLite in WAL mode, one writer thread. `bundled` compiles
  SQLite from the amalgamation that ships inside the crate, so the app doesn't depend on
  the system `libsqlite3` version.
- `libsqlite3-sys` is pinned explicitly so the bundled SQLite version can't drift with a
  `cargo update`.
- No optional rusqlite features beyond `bundled`. In particular, `load_extension` stays
  off, so the extension-loading API isn't exposed to Rust code. The compiled SQLite still
  has `SQLITE_ENABLE_LOAD_EXTENSION=1`, but nothing can call it.

**New crates in the lockfile**

| Built on macOS | Other targets only |
|---|---|
| `rusqlite`, `libsqlite3-sys`, `hashlink 0.12.2`, `fallible-iterator 0.3.0`, `fallible-streaming-iterator 0.1.9`, `vcpkg 0.2.15` (build-dep, no build.rs) | `hashbrown 0.16.1` (via hashlink), `rsqlite-vfs 0.1.1`, `sqlite-wasm-rs 0.5.5` (wasm only) |

## build.rs review

- **`libsqlite3-sys`** (853 lines). With `bundled`, it copies the pregenerated bindings
  and compiles `sqlite3/sqlite3.c` with `cc`. Compile flags include
  `SQLITE_DEFAULT_FOREIGN_KEYS=1`, `THREADSAFE=1`, FTS3/FTS5, JSON1, RTREE, STAT4 and
  `ENABLE_LOAD_EXTENSION=1`. **No network access, no process spawning** besides the C
  compiler. It reads a few env vars (`LIBSQLITE3_FLAGS`, `SQLITE_MAX_*`) that can change
  compile flags; none are set in this repo or CI.
- `rusqlite`, `hashlink`, `fallible-*`, `vcpkg`: no build.rs.

**Audit:** OSV (includes RustSec) reports no advisories for any of the new crates at
these versions. Past `libsqlite3-sys` advisories only affect much older releases.

**In-house:** not feasible. A SQLite binding is thousands of lines of unsafe FFI.

---

## Recommendation

| Group | Recommendation |
|---|---|
| `rusqlite =0.40.2` (`bundled`) + `libsqlite3-sys =0.38.2` | ✅ Approve |

## Lockfile check

Rerun on 2026-10-19 after the `Cargo.toml` change landed, using the same check as the M0
section. Each `registry+` package in `src-tauri/Cargo.lock` was looked up on crates.io
and its publish date compared with the 2026-10-05 cutoff. Result: **428 checked, 0 late**.
No downgrades were needed for M1. M3's TLS stack is checked when it lands; see
`DEPENDENCY_REVIEW_M3.md`.
