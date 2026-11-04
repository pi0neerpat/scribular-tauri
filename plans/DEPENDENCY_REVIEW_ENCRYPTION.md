# Dependency review: encryption at rest (parity spec 01)

**Date:** 2026-11-04
**Status:** Approved by Patrick 2026-11-04: both changes, and option A (legacy login keychain) for the key.
**Spec:** `plans/parity/01-encryption-audio-retention.md` ("New dependencies").
**Policy:** `../notes-assistant-electron/CLAUDE.md` § Dependency Policy (review, audit,
exact pin, in-house check).

**Version rule used:** the newest stable release published on or before 2026-10-21
(two weeks before today).

---

## Summary

| Change | New crates in lockfile | Recommendation |
|---|---|---|
| `rusqlite =0.40.2`: feature `bundled` → `bundled-sqlcipher` | 0 | ✅ Approve (deviation from the spec, see below) |
| `security-framework =3.7.0` as a direct macOS dependency | 0 (already in the tree via `native-tls`) | ✅ Approve |
| `bundled-sqlcipher-vendored-openssl` (the spec's choice) | `openssl-src` + `openssl-sys` build | ❌ Not needed |
| `keyring` | wrapper over `security-framework` | ❌ Not needed |
| `getrandom` / `rand` | – | ❌ Not added: 32 bytes from the Security framework's `SecRandomCopyBytes` (via `security-framework`) |
| `zeroize` | – | ❌ Not added: a best-effort overwrite loop, as the spec allows |

**Trial lockfile diff** (copy of `src-tauri`, both changes applied, `cargo metadata`):
one line, `security-framework` added to `scribular`'s own dependency list. No crate
is added, removed or changed in version.

## 1. SQLCipher: `bundled-sqlcipher`, not `bundled-sqlcipher-vendored-openssl`

The spec assumed `bundled-sqlcipher` links a system OpenSSL. On Apple targets it
does not. `libsqlite3-sys 0.38.2` `build.rs:184-249`: when no `OPENSSL_DIR` /
`OPENSSL_LIB_DIR` / `OPENSSL_INCLUDE_DIR` is set and the host and target are Apple,
it compiles SQLCipher with `-DSQLCIPHER_CRYPTO_CC` (Apple CommonCrypto) and links only
the system `Security` and `CoreFoundation` frameworks. `openssl-sys` is an optional
dependency enabled only by the `vendored-openssl` feature, so it never builds.

**Trial build** (`/tmp/sqlcipher-trial`: rusqlite `=0.40.2` + `bundled-sqlcipher`,
release profile, `OPENSSL_*` unset):

```
cipher_version=4.14.0 community provider=commoncrypto provider_version=61901.60.40
magic=false          # first 16 bytes are not "SQLite format 3\0"
canary_in_file=false # inserted text not found in the file after WAL mode + write
$ otool -L sqlcipher-trial
  /System/Library/Frameworks/Security.framework/Versions/A/Security
  /System/Library/Frameworks/CoreFoundation.framework/Versions/A/CoreFoundation
  /usr/lib/libSystem.B.dylib
```

No `libcrypto` / `libssl`, nothing outside `/System` and `/usr/lib`. This is the
"self-contained binary" condition from spec Decision 7, met without OpenSSL at all:
no Perl/make build step, no OpenSSL CVE stream to track, no 1-2 min of extra CI time.

**The one risk:** if a build machine sets `OPENSSL_DIR` (or the two `_LIB_DIR` /
`_INCLUDE_DIR` vars), `build.rs` switches to dynamically linking that OpenSSL, which
end users don't have. Guards, both in the implementation PR:
- Startup check: `PRAGMA cipher_provider` must be `commoncrypto` on macOS, alongside
  the spec's `cipher_version` check (R4). Test asserts it (AS3).
- Release workflow: the `otool -L` step fails the build on any non-system
  `libcrypto`/`libssl` (spec coordination note).

| Field | Value |
|---|---|
| Crates | `rusqlite =0.40.2`, `libsqlite3-sys =0.38.2` (already pinned, approved in M1) |
| SQLCipher | 4.14.0 community, vendored inside `libsqlite3-sys` |
| Licence | SQLCipher: BSD-style (Zetetic). Its notice must ship with the app; add it to the third-party notices with this change. |
| OSV / RustSec | `libsqlite3-sys 0.38.2`: no advisories (OSV query 2026-11-04) |
| build.rs | Same `libsqlite3-sys` script already reviewed in M1; the SQLCipher branch reads `OPENSSL_*` env vars, adds `-D` flags, emits two framework link lines. No network, no process spawning. |

`bundled` and `bundled-sqlcipher` can't sit side by side, so `bundled` goes. Every
caller keeps the same `rusqlite` API.

## 2. Keychain: `security-framework =3.7.0`

| Field | Value |
|---|---|
| Pin | `=3.7.0`, published 2026-02-20 (newest stable; inside the version rule) |
| Downloads | 119 M total, 110 M recent (90 d) |
| Owners | kornelski, djc |
| Licence | MIT OR Apache-2.0 |
| OSV / RustSec | clean (OSV query 2026-11-04) |
| Already in tree | yes: `native-tls 0.2.18` → `reqwest` uses it for TLS on macOS today |
| Placement | `[target.'cfg(target_os = "macos")'.dependencies]` |

Used for: `passwords::{generic_password, set_generic_password_options,
delete_generic_password_options}` with `PasswordOptions` (service = bundle id,
account = `database-key`), and `random::SecRandomCopyBytes` for the 32-byte key.
Spec 04 reuses the same type with account `license`; no separate review.

**In-house alternative:** raw `SecItemAdd` / `SecItemCopyMatching` FFI, about 80
lines of `unsafe` CoreFoundation dictionary building. More risk than a crate we
already ship.

## Open question for Patrick (affects spec R3 and Decision 2)

The spec wants the key "when-unlocked, this-device-only". On macOS that attribute is
only honoured by the **data-protection keychain**, not the legacy file-based login
keychain (Apple TN3137). Two options:

| | A. Legacy login keychain (recommended for now) | B. Data-protection keychain, `WhenUnlockedThisDeviceOnly` |
|---|---|---|
| Works in unsigned dev builds / `tauri dev` | yes (prompts after each rebuild) | likely no: the app must be signed with a keychain access group entitlement (team-prefixed), which may need a provisioning profile; to confirm in a trial signed build |
| Developer ID release | yes | entitlement added to the U11 signing setup, possibly a Developer ID provisioning profile; also enables `security-framework`'s `OSX_10_15` feature (no new crates) |
| iCloud sync of the key | never | never |
| Restore onto a new Mac | possible if the user migrates or restores their login keychain file (Time Machine / Migration Assistant) and knows its password | never: history can't move to another Mac |
| What the docs can say | "The key is in your macOS Keychain, not in the file. It is never sent to iCloud." | the spec's wording, including "Sessions cannot be moved to another Mac" |

Both keep the key out of the database file and out of iCloud. B is stricter but
needs signing infrastructure only Patrick can provide and blocks local dev builds
from opening the DB. A ships now; B can follow when the provisioning profile exists
(it would need a one-time key move, same account name).
