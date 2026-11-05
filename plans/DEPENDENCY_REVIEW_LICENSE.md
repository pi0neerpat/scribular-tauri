# Dependency review: licensing and free trial (parity spec 04)

Date: 2026-11-05. Spec: `plans/parity/04-licensing-trial.md`.

## New crates: none

Each need is met by code or crates already in the tree:

| Need | Choice | Why no new crate |
| --- | --- | --- |
| Keychain item for `{license_key, device_id, trial_started_at}` | Spec 01's `KeyStore` / `KeychainKeyStore` (`security-framework =3.7.0`, `plans/DEPENDENCY_REVIEW_ENCRYPTION.md`) | The trait gains two provided byte-blob methods (`get_bytes` / `set_bytes`), so one abstraction serves the database key and the licence item. |
| HTTPS POST to `/api/validate` | `reqwest =0.13.5` (`native-tls`, `plans/DEPENDENCY_REVIEW_M3.md`) | Already used for model download. The JSON body is built with `serde_json`, so the `json` feature is not needed. |
| UUIDv4 device ID | 16 bytes from `store::key::new_key()` (SecRandom on macOS), formatted in-house with the v4 version and variant bits | `uuid` is only a transitive dependency in `Cargo.lock`; ten lines beat a direct dependency. |
| RFC 3339 timestamps and the server's `expirationDate` | In-house civil-date conversion (Howard Hinnant's `days_from_civil`) | `chrono` and `time` are transitive only; the app needs UTC date arithmetic, not time zones. |
| Key format check `^[a-z]+-[a-z]+-[a-z]+-\d{3}$` | A hand-written matcher | Adding `regex` for one fixed pattern is not worth it. |
| Opening the pricing page | `std::process::Command::new("/usr/bin/open")`, URL parsed with `reqwest::Url` | No opener plugin; same approach as `open_privacy_settings`. |

No `Cargo.toml` or `package.json` change.
