# Dependency review: M0 (scaffold)

**Date:** 2026-10-19
**Status:** Approved 2026-10-19.
**Policy:** `../notes-assistant-electron/CLAUDE.md` § Dependency Policy (review, audit,
exact pin, in-house check).

**Version rule used:** the newest stable release that is at least 2 weeks old
(published on or before 2026-10-05). Waiting a short time gives malicious or broken
publishes time to be caught and pulled before we install them.

---

## npm packages

| Package | Pin | Published | Weekly DL | Stars | Maintainers | Install scripts |
|---|---|---|---|---|---|---|
| `@tauri-apps/api` | `2.12.1` | 2026-09-30 | 2.0 M | 112 k (tauri) | 7 | none |
| `@tauri-apps/cli` (dev) | `2.12.1` | 2026-09-30 | 1.9 M | 112 k (tauri) | 7 | none |
| `svelte` (dev) | `5.57.1` | 2026-09-18 | 4.2 M | 88 k | 3 | none |
| `@sveltejs/vite-plugin-svelte` (dev) | `7.3.1` | 2026-09-23 | 2.7 M | 1.0 k | 3 | none |
| `vite` (dev) | `8.3.2` | 2026-10-01 | 119 M | 83 k | 2 | none |
| `typescript` (dev) | `6.0.3` | 2026-04-16 | 184 M | 112 k | 7 | none |
| `svelte-check` (dev) | `4.7.6` | 2026-08-13 | 2.0 M | 1.4 k (language-tools) | 2 | none |
| `vitest` (dev) | `5.0.3` | 2026-09-30 | 78 M | 17 k | 5 | none |

**Notes**
- **TypeScript is pinned to 6.0.3, not the latest 7.0.2.** `svelte-check` 4.7.x declares
  `typescript: ^5 || ^6` as a peer dependency, so TS 7 isn't supported yet.
- `svelte` is a dev dependency because it compiles away at build time.
- The CLI and API are both pinned to `2.12.1` to match the Rust `tauri` crate below.
  Tauri warns when the JS and Rust versions differ.
- All repos were active within the last week and none are archived.
- 2–3 maintainers on `vite`, `svelte` and `svelte-check` is normal for org-run projects.
  Publishing is controlled by the Vite and Svelte organizations, not one person.

**Audit:** I built a lockfile with exactly these pins (`npm install
--package-lock-only --ignore-scripts`), then ran `npm audit`:
- **0 vulnerabilities**, 98 packages in total including dependencies of dependencies.
- **Install scripts in the full tree:** `fsevents` only. It's the optional macOS
  file-watcher used by Vite's dev server. Version 2.x ships prebuilt binaries, and its
  script only builds from source (node-gyp) if those are missing. Low risk.
- I don't know of any supply-chain incidents for these packages.

**In-house:** not feasible. These are the framework, build tool and type checker.

---

## Rust crates

| Crate | Pin | Published | Recent DL (90 d) | Stars | Owners | OSV / RustSec |
|---|---|---|---|---|---|---|
| `tauri` | `=2.12.1` | 2026-09-30 | 15.0 M | 112 k | `tauri-bot` (org release bot) | clean (old advisories fixed in 2.x) |
| `tauri-build` | `=2.7.1` | 2026-09-30 | 15.0 M | 112 k | `tauri-bot` | clean |
| `serde` | `=1.0.229` | 2026-07-18 | 371 M | 11 k | dtolnay + serde-rs org | clean |
| `serde_json` | `=1.0.151` | 2026-07-20 | 370 M | 5.7 k | dtolnay + serde-rs org | clean |
| `specta` | `=2.0.0-rc.25` | 2026-05-07 | 1.4 M | 646 | **oscartbeaumont (1)** | clean |
| `tauri-specta` | `=2.0.0-rc.25` | 2026-05-08 | 0.7 M | 810 | oscartbeaumont, Brendonovich | clean |
| `specta-typescript` | `=0.0.12` | 2026-05-07 | 0.8 M | (specta repo) | **oscartbeaumont (1)** | clean |

**Notes**
- `tauri-build` is versioned separately from `tauri`. `2.7.1` was released together
  with `tauri 2.12.1`.
- The past `tauri` advisories (filesystem scope bypass, origin confusion, iframe IPC
  access) all affect older versions. OSV reports none for `2.12.1`.
- `cargo-audit` couldn't run because **no Rust toolchain is installed on this
  machine**. Crates were checked against the OSV database instead, which includes
  RustSec. M0 needs `rustup` installed. That's a toolchain, not a project dependency,
  but it's listed here so the approval covers it.

### ⚠️ Flag: specta / tauri-specta

- **Pre-release only.** The 2.x line has been a release candidate since 2024 and is now
  at rc.25. The only stable versions (1.x, from 2023) don't support Tauri 2.
- **Few maintainers.** `specta` and `specta-typescript` each have one owner on crates.io.
- `tauri-specta` requires exactly `specta =2.0.0-rc.25`, so all three must be upgraded
  together.
- **It does very little the MVP needs.** About 15 commands and 3 event types.

**In-house alternative (recommended):**
1. Write `src/lib/api.ts` by hand: typed `invoke` and `listen` wrappers for each command
   and event, with TS interfaces matching the Rust `serde` structs.
2. Add a Rust unit test that lists every command registered in `generate_handler![]`
   and fails if any is missing from `api.ts`. That catches the "renamed command fails
   silently" bug that typed IPC was meant to prevent.

That's roughly 60 lines of TypeScript plus a 20-line test, well under the policy's
~300-line threshold. If the command list grows a lot after the MVP, look at specta
again; it may have a stable release by then.

### Alternatives to specta

| Crate | Stable pin | 90 d DL | Stars / contributors | Owners | Generates | Verdict |
|---|---|---|---|---|---|---|
| **`ts-rs`** | `=12.0.1` (2026-01-31) | 8.2 M | 1.9 k / 56 (Aleph-Alpha org) | NyxCode (1 on crates.io) | TS types from Rust structs/enums | ✅ **Recommended** |
| `typeshare` | `=1.0.5` (2026-01-02) | 1.7 M | 3.0 k / 47 (1Password) | 1Password publisher + 1 | TS (and Swift/Kotlin) types, via a separate CLI | Workable, but needs the `typeshare-cli` binary (30 k downloads) as an extra build step |
| `taurpc` | `0.8.2` | 2.5 k | 332 / 9 | 1 | Commands + types | ❌ Built on `specta =2.0.0-rc.25`, so it has the same problem |
| `tauri-bindgen` | unpublished | — | 196 / 8 | — | Commands + types (WIT) | ❌ Experimental, not on crates.io |

**`ts-rs` details**
- Only 2 runtime dependencies: `thiserror` (already likely) and its own macro crate,
  pinned exactly to the same version. OSV is clean for both.
- No `build.rs`. `#[derive(TS)] #[ts(export)]` writes `.ts` files when `cargo test`
  runs, so type generation fits into the test run we already have.
- It supports `serde` attributes (`rename_all`, `tag`), so the generated TS matches
  what Tauri actually serializes.
- It's stable (12.x) with semver releases. The single crates.io owner is a risk, but
  the code is maintained in an org repo with 56 contributors and could be forked
  easily.

**What it covers and what stays in-house**
- Generated: every type that crosses IPC (command arguments and return values, the
  `session://progress` and `session://segments` event payloads, `Session`, `Prompt`,
  `Settings`, `JobState`).
- Still hand-written: thin `invoke` and `listen` wrappers in `api.ts` (about 1 line per
  command, using the generated types), plus the command-list test from above.
- This removes about two thirds of the in-house code, and all of the part where
  mistakes happen (keeping the type shapes in sync).

---

## Recommendation

| Group | Recommendation |
|---|---|
| npm (8 packages) | ✅ Approve at the pins above |
| `tauri`, `tauri-build`, `serde`, `serde_json` | ✅ Approve at the pins above |
| `specta`, `tauri-specta`, `specta-typescript` | ❌ Don't add |
| `ts-rs` `=12.0.1` (+ `ts-rs-macros` `=12.0.1`) | ✅ Approve. Generates the IPC types; thin hand-written `api.ts` wrappers plus the command-list test cover the rest |
| `rustup` toolchain | ✅ Install (stable channel), required for M0 |

If approved, I'll update `TAURI_MVP.md` (the IPC-types row in Decisions and section 5)
to use `ts-rs` + thin wrappers.

---

## Lockfile check (U1 scaffold, 2026-10-19)

Exact pins on direct dependencies don't fix the transitive tree. `tauri = "=2.12.1"`
asks for its own internals with `~` ranges, so a fresh lockfile resolved them to
releases newer than the rule allows. I checked the publish date of every locked
package (crates.io API and npm registry) against the 2026-10-05 cutoff.

**npm:** 19 of 98 were too new, all under three roots. Pinned with `overrides` in
`package.json`:

| Package | Resolved | Pinned | Required by |
|---|---|---|---|
| `rolldown` (+ 15 platform bindings, `@oxc-project/types`) | 1.2.14 | 1.2.12 | `vite` `~1.2.11` |
| `postcss` | 8.5.31 | 8.5.29 | `vite` `^8.5.28` |
| `tinybench` | 6.2.2 | 6.2.0 | `vitest` `^6.1.4` |

After: 98 packages, 0 past the cutoff, `npm audit` 0 vulnerabilities.

**Cargo:** 35 of 409 were too new. Each was moved with `cargo update -p <crate>@<ver>
--precise <newest compatible version on or before the cutoff>`, repeated until a full
recheck was clean (three passes, because Cargo refuses a downgrade while another locked
crate still needs the newer version). Notable ones:

- Tauri internals released a week after `tauri` 2.12.1: `tauri-runtime` and
  `tauri-runtime-wry` 2.12.2 → 2.12.1, `tauri-utils` 2.10.2 → 2.10.1, `tauri-macros`
  and `tauri-codegen` 2.7.3 → 2.7.1.
- Build tooling: `cc` 1.9.0 → 1.6.0, `swift-rs` 1.0.9 → 1.0.8, `embed-resource`,
  `tauri-winres`.
- Runtime: `tokio` 1.54.0 → 1.53.2, `hyper` 1.12.0 → 1.11.1, `libc`, `uuid`, `toml` and
  its parts, `syn` 3.0.9 → 3.0.6.

After: 419 crates (older versions pull in a few more), 0 past the cutoff. All U1 checks
pass and the app runs on the downgraded tree.

**For later units:** rerun this check whenever `Cargo.lock` or `package-lock.json`
changes.

## ts-rs on the dev VM

ts-rs's own exporter calls `File::sync_all`, which on macOS issues
`fcntl(F_FULLFSYNC)`. The VirtioFS share the repo lives on rejects that with `ENOTTY`
(a plain `fsync` works), so `#[ts(export)]` fails here even though the files get
written. On a normal APFS disk it works.

Workaround, with no new dependency: the types aren't marked `#[ts(export)]`. A
`cargo test` (`commands::tests::export_bindings`) has ts-rs export every root type
listed in `export_ipc_types` into a temp dir, then copies the result into
`src/lib/types/` with plain writes and deletes generated files that no longer match a
type. New IPC types must be added to `export_ipc_types`.
