# Encryption at rest, keep-audio, audio auto-delete and playback — Tauri parity spec
Status: draft for cwf; Date: 2026-11-03

## Intent (why users need it; who)
Therapists will put real client sessions into Scribular. Three promises the Electron app makes, and the Tauri app must keep before any real data goes in:
1. Session text (transcripts, summaries) is encrypted on disk.
2. Audio is thrown away by default. If the user opts in ("Save session audio"), audio is kept encrypted, for a limited time, and can be replayed to check a transcript.
3. Kept audio expires on its own, and the UI says when.

The Tauri prototype deliberately breaks all three (prototype plan R8, KTD1, KTD2: plain SQLite, audio always deleted, "test and demo sessions only, never real clients"). This spec removes that restriction. New users only; a few testers hold a prototype database that must be handled safely.

## Electron behaviour today (with file:line refs)
Read from `notes-assistant-electron` (v1.5.10). Where behaviour is a bug or weakness it is marked **[weak]**.

**Session encryption**
- Sessions live in an `electron-store` JSON file `userData/data/sessions.json`, max 100 sessions (`src/store/sessions.js:28-36`, `:268-271`).
- Field-level encryption with `safeStorage.encryptString` (macOS Keychain "Safe Storage" key). Encrypted: `summary`, `transcript`, `previousSummary`, `rawTranscript`, `plainTranscript`, `speakerMap` (`sessions.js:46-66`, `:208-260`, `:404-477`).
- **[weak]** Plaintext metadata: `id`, `timestamp`, `duration`, `promptId`, `previousPromptId`, `model`, `previousModel`, `hasAudio`, `isDiarized` (`sessions.js:218-259`; `model` comment at `:255-256`).
- **[weak]** Fail-open: if `safeStorage.isEncryptionAvailable()` is false, `encryptData` returns the plaintext and the app only logs "Encryption not available" (`sessions.js:19-23`, `:50-52`). The user is never told. `encryptData` also returns objects unencrypted if they stringify to `[object Object]` (`:56-58`).
- **[weak]** Decrypt failure returns `""` (`sessions.js:112-115`), so a lost key looks like an empty session.
- Deleting a session removes its audio file; clear-all removes all audio files (`sessions.js:284-338`).

**Keep-audio setting**
- `saveAudio`, default `false` (`src/store/defaults.js:79`). Settings > Audio Recording > "Save session audio" Off/On toggle (`src/index.html:551-565`; `src/renderer/views/settings.js:770`, `:895`, `:911`). Copy: "Session recordings stay only on your device for 7 days, then are permanently deleted. Enable only if you need to verify transcript accuracy." (`index.html:557`).
- Evaluated once at recording start: if on, a plaintext temp WAV path `os.tmpdir()/scribular_<id>_raw.wav` is passed to the worker (`src/service/audio/audio.js:560-573`). Toggling during a recording has no effect on that recording.
- The worker writes the merged mono WAV (system + mic summed, clamped to ±1; `src/service/audio/saveAudio.js:12-60`) to that temp path and posts `audioSaved` (`audioWorker.js:44-72`, calls at `:523`, `:601`, `:665`).
- Main process then reads the temp WAV, deletes it, encrypts with AES-256-GCM (`[12B iv][16B tag][ciphertext]`, `audioEncryption.js:52-58`), writes `userData/audio/<sessionId>.wav.enc`, and sets `hasAudio: true` (`sessionAudioPersistence.js:15-44`; `audio.js:97-103`).
- Key: random 32 bytes, stored in app config as a `safeStorage`-encrypted base64 string `audioEncryptionKey` (`audioEncryption.js:16-43`; `src/store/store.js:110`, `:141-142`).
- **[weak]** If the key fails to decrypt it is silently regenerated (`audioEncryption.js:32-35`), orphaning all existing `.wav.enc` files with no user-visible error.
- **[weak]** Plaintext audio touches disk twice: the temp `_raw.wav` during and after recording until encryption finishes, and the `_playback.wav` temp file during playback. A crash leaves them in `tmpdir`; they are only swept on next launch (`src/index.js:400-413`, called `:437`).
- **[weak]** Audio is only saved if the recording finishes normally; a crash loses it (and the orphan temp WAV is deleted at next launch, `index.js:404-409`).
- The audio write error path only logs; the session is saved without `hasAudio` and the user is not told (`audio.js:97-103`).

**Auto-delete / retention display**
- Retention is a fixed 7 days measured from `session.timestamp` (`src/index.js:354`, `:383-387`). Not configurable.
- `cleanupExpiredAudio()` runs once at app start (`index.js:436`). **[weak]** A long-running app does not expire audio until relaunch, so "deleted after 7 days" can be late.
- It also (a) clears `hasAudio` on sessions whose file is missing (`:359-367`), and (b) unlinks orphan `.wav.enc` files with no session (`:377-381`). File deletion is `fs.unlinkSync`, not a secure erase.
- UI: when a session with audio is opened, the player shows "Recording deletes <Mon D>" computed in the renderer as `timestamp + 7d` (`src/renderer/views/session.js:452-465`; CSS `session.css:464`). It is a pure display value, not read from the file.

**Session audio playback**
- `session.js:441-475` `showAudioPlayer`: invoke `SESSION_REQUEST_AUDIO` (`session:requestAudio`, `src/common/ipcChannels.js:114`; preload `src/preload/preload.js:145`). On `{error}` or no URL, the player container is hidden (silent). On success an `<audio controls>` is inserted into `#session-audio-player` (`index.html:376`). Called at `session.js:833`.
- Main handler (`src/main/registerIpcHandlers.js:201-230`): path-traversal guard, decrypts the whole file into memory, writes plaintext `os.tmpdir()/scribular_<id>_playback.wav`, returns `file://` URL. Errors: `"Invalid session ID"`, `"Audio file not found"`, or the exception text.
- `hideAudioPlayer` (`session.js:481-492`, called `:962` when leaving the session) calls `SESSION_RELEASE_AUDIO` (`ipcChannels.js:115`; preload `:146`), which deletes the temp file (`registerIpcHandlers.js:233-248`, with traversal guard).
- **[weak]** If the window is closed or the app quits while playing, no release runs; the file stays in tmpdir until the next launch sweep.
- **[weak]** There is no per-session "delete the recording now" action. Only whole-session delete, clear-all, or the 7-day sweep remove audio.
- E2E-only fixture paths exist (`registerIpcHandlers.js:127-140`, `:210-218`; `audio.js:110-114`). They are test scaffolding. Do not port.

## Tauri state today (what exists, what is missing, with file refs)
- Store: plain `rusqlite = { version = "=0.40.2", features = ["bundled"] }` (`src-tauri/Cargo.toml:26`), WAL, FKs on, `secure_delete` on, one `Mutex<Connection>` (`src-tauri/src/store/mod.rs:64-84`). DB at `app_data_dir()/scribular.db` (`src-tauri/src/lib.rs:71-73`). `Store::open(path)` also called from `src-tauri/src/bin/bench.rs:176` and tests.
- Everything unencrypted, including `title`, `transcript`, `summary`, `segments.text`, `audio_chunks.pcm` (`store/migrations.rs:42-93`).
- Migrations: ordered, append-only, `PRAGMA user_version`, currently 3 (`migrations.rs:11`, `:25-38`). `SchemaTooNew` guard exists (`mod.rs:27-31`).
- Audio: captured to 16 kHz mono i16 BLOB rows, one stream per `AudioSource`, in `audio_chunks` during recording (`migrations.rs:67-73`; `store/sessions.rs:518-565`). System audio arrives over the AudioCapCLI sidecar's stdout pipe (`audio/system.rs:94-116`), not via files. So no plaintext audio file hits disk in the capture path. Good; keep it that way.
- Audio is deleted in the same transaction as the transcript save (`store/sessions.rs:451-468`, called from `jobs/session.rs:607`). No `keep_audio` setting, no `audio_kept` column (dropped by KTD2).
- Sessions left `Recording`/`Finalizing` at launch become `Failed("interrupted")` and keep their audio rows (`store/sessions.rs:567-595`). **Nothing ever deletes those rows except deleting the session.** Under this spec that becomes a retention hole and must be closed (R16).
- `delete_session` relies on `ON DELETE CASCADE` (`sessions.rs:470-474`). No clear-all command yet (spec 09).
- `Session`/`SessionListItem` carry no audio flag (`sessions.rs:111-137`). `RecordingSettings` holds input mode, target app, prompt id only (`store/settings.rs:18-22`).
- No playback command, no player component. CSP is `default-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:` (`tauri.conf.json:21`), which blocks `blob:` media.
- Settings UI (`src/lib/components/Settings.svelte`) has no audio-retention section. No encryption or retention copy anywhere in `src/`.
- No crate for Keychain, no SQLCipher. `plans/TAURI_MVP.md:82-83,110-128` and `:271-272` already chose SQLCipher + Keychain + PCM-in-DB audio; this spec implements that design with the corrections below.

## Goals / Non-goals / Anti-goals
**Goals**
- The entire database (and its WAL/SHM) is SQLCipher-encrypted with a random 256-bit key in the macOS Keychain. Nothing session-related is readable from the files at rest.
- The app **fails closed**: if it cannot get or apply the key, it does not run unencrypted.
- "Save session audio" setting, default off. Kept audio lives only inside the encrypted DB.
- Kept audio is deleted automatically 7 days after the session started, with the date shown.
- In-app playback of kept audio with no plaintext audio ever written to disk.
- A tester's unencrypted prototype DB is converted safely, or the user is told clearly what happened.
- Every security claim in UI copy and docs is true and bounded (see "Security claims").

**Non-goals**
- Migration of Electron data (decided in brief).
- A user-configurable retention period (Electron is fixed at 7 days).
- Password / passphrase / Touch ID app lock.
- Exporting audio. Per-session "delete audio only" button (not in Electron; see Decisions).
- Encrypting models or app settings files (not client data).
- Windows/Linux key storage.

**Anti-goals**
- Do not write decrypted audio (or transcripts) to temp files, the webview cache, localStorage, or logs.
- Do not add an "encryption off" switch or a silent fallback to a plaintext DB.
- Do not regenerate the key over an existing encrypted DB (Electron's `audioEncryption.js:32-35` mistake).
- Do not market this as "HIPAA compliant" or "secure" in unqualified terms.
- Do not invent a custom crypto format for audio. The DB layer encrypts everything.

## Requirements (numbered R1..)

### Encryption at rest
- **R1.** The database file is encrypted with SQLCipher 4 defaults. The first 16 bytes of `scribular.db` are never `SQLite format 3\0`. The `-wal` and `-shm` files contain no plaintext (SQLCipher encrypts WAL frames; verified by test, AS2).
- **R2.** The key is 32 bytes from the OS CSPRNG, applied as a raw key (`PRAGMA key = "x'<64 hex>'"`). Raw-key mode is deliberate: the key is random, so a passphrase KDF adds only open-time cost. `PRAGMA key` must be the first statement on the connection, before `journal_mode`.
- **R3.** The key is stored in the login Keychain as a generic password: service = bundle identifier (`com.app.scribular.tauri` today, `tauri.conf.json:5`; read it from config, do not hard-code a second copy), account = `database-key`. Accessibility: when-unlocked, this-device-only (no iCloud sync, not carried by backups). Never write the key to disk, logs, diagnostics, settings, env vars or command-line args. Do not shell out to `/usr/bin/security` (the key would appear in `ps`).
- **R4.** A startup self-check, before any other DB use: `PRAGMA cipher_version` must return a non-empty string, otherwise the build is not linked against SQLCipher and `PRAGMA key` is silently ignored (plain SQLite does that, producing a plaintext DB). On failure: refuse to open and show a fatal error. This guards against a mis-set Cargo feature.
- **R5.** Open decision table (the state machine is the contract; implement as a pure function over `{db_file: absent|plaintext|encrypted, keychain: absent|present}` so it is unit-testable):

| DB file | Keychain key | Action |
|---|---|---|
| absent | absent | generate key, store it, **verify by reading it back**, then create encrypted DB |
| absent | present | reuse key, create DB (orphan key from an earlier crash) |
| encrypted | present, opens OK | normal |
| encrypted | present, fails to open ("file is not a database") | **Locked state** (R7). Never overwrite |
| encrypted | absent | **Locked state** (R7). Never generate a new key |
| plaintext (prototype) | any | migrate (R8-R10) |
| plaintext, migration fails | any | **Locked state** variant "migration failed"; the plaintext file is left untouched; the app does not open it for use |
| present but not SQLite and not decryptable | any | Locked state |

- **R6.** Any Keychain failure (access denied by user, locked keychain, item unreadable, write failure) maps to the Locked state with a specific message. There is no fallback to a file-stored key.
- **R7.** Locked state: the Rust core does not start recording, queues or any `Store`-using command. Commands return a typed `StoreLocked { reason }` error. The webview shows a blocking screen with the reason, a "Quit" button, and (only for the "key missing / cannot decrypt" reasons) a "Start fresh" button. "Start fresh" requires a typed-confirmation dialog stating that the old sessions cannot be recovered, renames the old DB to `scribular.db.unreadable-<unix-ms>` (still encrypted, nothing is deleted), and creates a new DB with a new key. Keep the renamed file out of the retention sweep (it is not touched).
- **R8.** Prototype-DB migration, in place, only when the file starts with the SQLite magic header **and** opens without a key and `user_version` is 1..=known:
  1. Checkpoint WAL (`PRAGMA wal_checkpoint(TRUNCATE)`) on the plaintext DB, close.
  2. Create `scribular.db.migrating` encrypted with the new key; use `ATTACH ... KEY` + `SELECT sqlcipher_export('encrypted')`; set `PRAGMA encrypted.user_version` to the source version (export does not copy it).
  3. Open the new file with the key, run `PRAGMA integrity_check` (must be `ok`) and compare row counts of `sessions`, `segments`, `audio_chunks`, `prompts`, `settings` with the source.
  4. Only then: rename `scribular.db` → `scribular.db.plaintext-old`, rename `.migrating` → `scribular.db`, fsync the directory, and run the remaining normal migrations.
  5. Delete `scribular.db.plaintext-old`, `-wal`, `-shm` (R9).
  A stale `.migrating` file at startup is deleted and the migration restarts. A crash between any two steps leaves either the original plaintext DB (restart migrates again) or the finished encrypted DB plus a leftover `.plaintext-old` (deleted on next start). Use the decision table to recognise each state.
- **R9.** Removing the plaintext copy: overwrite with zeros in place, `fsync`, then unlink; same for `-wal`/`-shm`. This is **best effort and must be described as such**. On APFS (copy-on-write) with SSD wear levelling, overwriting does not guarantee the old blocks are gone. The real protection for those old blocks is FileVault. Log (without content) whether the overwrite succeeded. The migration result is surfaced once in the UI: "Your earlier test sessions were encrypted. Turn on FileVault to protect copies of the old file that macOS may still hold."
- **R10.** Migrated sessions keep their data. Prototype sessions of status `Failed("interrupted")` keep their audio rows until the normal retention sweep (R14) removes them (7 days from `created_at`, so old ones go on the first sweep).
- **R11.** The DB layer is encrypted wholesale, so there is no per-field policy. No plaintext metadata remains (unlike Electron). Non-database files in `app_data_dir` (models, logs) are not client data, and **nothing client-derived may be written there**: transcripts, summaries and prompt inputs must not appear in logs or diagnostics (cross-check spec 06; add a test, AS11).
- **R12.** `Store::open` becomes `Store::open(path, &dyn KeyStore)`. `KeyStore` is a small trait (`get() / set()`), with a macOS Keychain implementation (`KeychainKeyStore::new(service, account)`; service = the bundle identifier, account `db-key`, so spec 04 reuses the same type with account `license`) and an in-memory fake for tests and `bin/bench.rs` (engines-behind-traits convention). Real Keychain is never touched from `cargo test`.

### Keep-audio setting
- **R13.** Add `keep_audio: bool` (default `false`) to `RecordingSettings` (ts-rs type regenerated; `set_recording_settings` persists it in the `settings` table). Settings view gets an "Audio recording" section with an Off/On control and this copy (exact claims; do not weaken or extend): *"Save session audio. Recordings are stored encrypted on this Mac and deleted automatically 7 days after the session. Off by default. Turn on only if you need to check a transcript against the recording."* A second line under the control when On: *"Recordings are removed with the session if you delete it."* (true via cascade, R18/AS16.)
- **R13a. Evaluation point.** Audio is always written to `audio_chunks` while recording (crash safety, as today). Keep vs delete is decided when the transcript is saved (`save_transcript`, `store/sessions.rs:454`), by reading the setting **then**. Spec author's call: this errs toward deleting (turning the setting off mid-recording takes effect), unlike Electron's start-time check (`audio.js:564`).
- **R13b.** When kept, set `sessions.audio_kept = 1` (new column) in the same transaction as the transcript save; when not kept, delete the rows in that same transaction (existing behaviour). `audio_kept` is the single source of truth for "has audio"; `has_audio` exposed to the UI is `audio_kept = 1 AND EXISTS(audio_chunks)`.
- **R13c.** Cancelled-during-Finalizing sessions (`jobs/session.rs:621-623`) save a partial transcript; they follow the same rule.
- **R14.** Turning the setting **off** does not touch audio already kept (Electron parity; each recording was consented to at the time). Spec author's call. The UI line next to the toggle says: "Existing recordings are deleted on their own schedule." If Patrick prefers delete-on-disable, see Decisions D3.

### Auto-delete and retention display
- **R15.** Retention is 7 days from `sessions.created_at` (Electron measures from `timestamp`, i.e. session start; same anchor). Constant `AUDIO_RETENTION_DAYS = 7` in one place, shared by the sweep, the expiry value sent to the UI, and the copy.
- **R16.** The sweep deletes `audio_chunks` rows for every session with `created_at < now - 7d` and sets `audio_kept = 0`. It does **not** delete sessions or text. It skips only the session the `Recorder` currently holds (the live recording/finalizing one). Non-live `Recording`/`Finalizing` rows cannot exist after open, because `recover_interrupted` converts them at store open (`store/mod.rs:82`), so they are swept like any other row. It **does** cover sessions that never reached "kept" status: failed/interrupted sessions with retained rows (prototype leftovers and crash-recovery audio). Interaction with spec 09: crash recovery has 7 days from `created_at` to turn interrupted audio into a transcript. After that the audio is gone.
- **R17.** The sweep runs at app start (after the store opens and recovery runs), **then every hour while the app is running** (deliberately better than Electron's launch-only, `index.js:436`), and **once more lazily before any playback request** (R20). All in one transaction, followed by `PRAGMA wal_checkpoint(TRUNCATE)` so deleted pages leave the WAL. Orphan rows cannot exist (`ON DELETE CASCADE`, FKs on), so no orphan pass is needed, but the sweep asserts that with a count query in tests.
- **R18.** `delete_session` (and spec 09's clear-all) removes audio via cascade. Space is not returned to the OS by deletes. After clear-all, run `VACUUM` (SQLCipher supports it). `secure_delete` stays on so freed pages are zeroed before encryption anyway.
- **R19.** Retention display: when `has_audio`, the session view (the current-session result until spec 09 adds session detail) shows "Recording deletes <Mon D>" (same format as `session.js:453`), computed in Rust as `audio_expires_at = created_at + 7d` and sent on `Session`, so the UI never recomputes the policy. For the final day show "Recording deletes today" (Spec author's call; Electron shows only the date).

### Session audio playback
- **R20.** Two commands mirror the Electron pair, adapted so no plaintext touches disk:
  - `request_session_audio(id) -> Response` (raw bytes, `tauri::ipc::Response`, not a JSON array): runs the lazy sweep for that session, then reads all `audio_chunks` for the session, **mixes the system and mic streams by summing and clamping to the i16 range** (matches `saveAudio.js:37-38`), wraps it in a 16 kHz mono 16-bit PCM WAV header **in memory**, and returns the bytes. Errors: typed `AudioError::{NotFound, Expired, Locked}`. A session with `audio_kept = 0` or no rows → `NotFound`.
  - `release_session_audio(id)`: **not added** (Spec author's call, Design notes). Electron needed it only to delete a temp file; here nothing touches disk, so the frontend's `URL.revokeObjectURL` is the release. Only `request_session_audio` gets a command + `api.ts` wrapper.
  - The webview wraps the bytes in a `Blob` + `URL.createObjectURL`, assigns it to `<audio controls>`, and calls `URL.revokeObjectURL` and drops the element when the user leaves the session, deletes the session, or the expiry passes.
- **R21.** CSP gets `media-src blob:` and nothing broader. No asset-protocol scope, no `file:` URLs, no new capability for the webview.
- **R22.** Frontend failure behaviour (improvement on Electron's silent hide, `session.js:447-449`): `NotFound` hides the player; `Expired` shows "This recording has been deleted"; `Locked`/other errors show an inline "Couldn't load the recording" message. Never an unhandled exception.
- **R23.** A session id is a UUID/opaque string used only as a SQL parameter. There is no path construction from it anywhere in this feature (so the Electron path-traversal guards, `registerIpcHandlers.js:205-208`, have no equivalent; the test in AS16 asserts hostile ids are inert).
- **R24.** Memory ceiling: 16 kHz mono i16 is about 115 MB/hour as WAV; a 3-hour session is about 350 MB through IPC plus a blob copy. Acceptable for v1 (Spec author's call). The command returns `AudioError::TooLarge` above a 4-hour cap rather than risking a webview OOM; the UI shows "Recording too long to play in the app".

## Acceptance scenarios (numbered AS1..; Given/When/Then; each tagged automated or manual)

**Encryption**
- **AS1 (automated, cargo test).** Given a fresh data dir and a `MemoryKeyStore`, when `Store::open` runs and a session with transcript text `"ZEBRA-CANARY-7731"` is created, then `scribular.db` first 16 bytes ≠ `SQLite format 3\0`, and a byte search of `scribular.db`, `scribular.db-wal` and `scribular.db-shm` finds no `ZEBRA-CANARY-7731` (checked both before and after `wal_checkpoint`).
- **AS2 (automated).** Given a DB written with key A, when opened with key B, or without a key, then `Store::open` returns a typed locked error and the file's bytes are unchanged (hash before == after).
- **AS3 (automated).** `PRAGMA cipher_version` is non-empty in the test binary; a test that links the build's SQLite and fails if the version is empty (guards R4 and the Cargo feature).
- **AS4 (automated).** Decision-table test: every row of the R5 table, via a fake key store and fake file states. In particular *encrypted + key absent* never calls `KeyStore::set`, and never creates or modifies a DB file.
- **AS5 (automated).** Fresh start where `KeyStore::set` fails → locked state, no DB file created.
- **AS6 (manual, real app).** Given a clean macOS user account (no Scribular Keychain item), when the app first launches and a session is recorded, then Keychain Access shows exactly one item for the bundle id with account `database-key`; quitting and relaunching shows the session without a password prompt.
- **AS7 (manual).** Given the app has data, when the Keychain item is deleted in Keychain Access and the app is relaunched, then the Locked screen appears, no new DB is created, "Start fresh" needs the typed confirmation, and afterwards `scribular.db.unreadable-*` exists and the app works with an empty history.
- **AS8 (manual).** Given a Developer-ID-signed build is updated in place by a build signed with the same identity, then no Keychain prompt appears after the update. (Unsigned dev builds will prompt; that is expected and not a failure.)

**Prototype-DB migration**
- **AS9 (automated).** Given a fixture prototype DB (user_version 3, 3 sessions including one `Failed("interrupted")` with audio rows, one Done with transcript, one with segments), when the new build opens it, then the result is encrypted (AS1 checks), `user_version` equals the latest, all session text/segment/prompt/settings counts match, `integrity_check` is `ok`, and `scribular.db.plaintext-old`, `-wal`, `-shm`, `.migrating` do not exist afterwards.
- **AS10 (automated).** Crash-injection: for each step boundary of R8, simulate a crash (fixture state on disk), then run `open`; the end state is always one valid encrypted DB with all data or the original plaintext DB untouched and the Locked "migration failed" state. Never both a usable plaintext DB and an encrypted one in use. A corrupt plaintext source → locked "migration failed", source untouched.
- **AS10b (manual, real app).** Given a tester's real prototype DB, when the newer build is installed over it, then sessions are visible, the one-time notice about FileVault appears once, and the DB header check (`head -c 16 scribular.db | xxd`) shows no SQLite magic.
- **AS11 (automated).** A test captures stderr and the log buffer while a session with canary text is created, summarised (fake engines), regenerated and deleted, and fails if the canary appears. (The same guard is reused by spec 06.)

**Keep-audio and retention**
- **AS12 (automated, fake engines).** Given `keep_audio=false` (default), when a recording completes and the transcript is saved, then `audio_chunks` has 0 rows for the session and `audio_kept=0`. Exactly the same for Cancelled-during-Finalizing and for a failed summary.
- **AS13 (automated).** Given `keep_audio=true`, when the transcript is saved, then audio rows remain, `audio_kept=1`, `audio_expires_at = created_at + 7d`. Given the setting is flipped to `false` before `save_transcript` runs, the audio is deleted (R13a).
- **AS14 (automated).** Given kept audio on sessions aged 6d23h and 7d1h, and a prototype `Failed("interrupted")` session aged 8d with audio rows, when the sweep runs, then only the 7d1h and 8d sessions lose their rows; text and segments remain; a live `Recording` session of any age is untouched. Running the sweep twice is idempotent.
- **AS15 (automated).** Given the app has been up past an expiry boundary (injected clock), when the hourly tick fires then the audio is gone without a restart; and `request_session_audio` on an expired-but-unswept session returns `Expired` and deletes the rows.
- **AS16 (automated).** `delete_session` removes the audio rows; after clear-all `VACUUM` runs; `request_session_audio("../../etc/passwd")`, `"' OR 1=1 --"` return `NotFound`, touch no file, and corrupt nothing.
- **AS17 (manual, real app).** Settings shows the toggle Off by default on first run, with the R13 copy; flipping it persists across relaunch.

**Playback**
- **AS18 (automated).** Given a session with system and mic chunks of known samples, `request_session_audio` returns a valid 16 kHz mono 16-bit WAV (header fields + length), samples equal the clamped sum, and a mic-only session returns the mic samples. Over the 4h cap returns `TooLarge`.
- **AS19 (automated, vitest).** The player component: on bytes it creates a blob URL and an `<audio controls>`; on `NotFound` it is hidden; on `Expired` it shows the deleted message; on unmount or navigating to another session it calls `URL.revokeObjectURL`.
- **AS20 (manual, real app).** With keep-audio on, record 30 s of a YouTube tab and the microphone, stop, wait for the summary: the session shows a player and "Recording deletes <date 7 days out>"; play, seek, hear both sources. While it plays, `lsof -p <pid>` and `ls $TMPDIR ~/Library/Application\ Support/com.app.scribular.tauri` show **no** new `.wav` file and a byte search of `~/Library/Application Support/com.app.scribular.tauri` and `$TMPDIR` for the WAV `RIFF` header finds nothing. Leaving the session (starting a new recording, or opening Settings, until spec 09 adds navigation) ends playback and releases the blob.
- **AS21 (manual).** With keep-audio off (default), the same recording shows no player and no "deletes" line. Force-quit mid-recording, relaunch: the session is `Failed("interrupted")`, its audio is retained for recovery (spec 09) and is gone after the 7-day sweep (set the system clock forward in a throwaway VM, or use the injected-clock test AS14 as the primary evidence).
- **AS22 (manual).** With keep-audio on, set the clock 8 days forward (throwaway VM / test user), relaunch: transcript and summary remain, the player is gone, audio rows are gone. (Before spec 09 there is no way to reopen an old session in the UI: verify rows/text with the AS14/AS15 automated tests plus `get_session` from the devtools console; the "player is gone" check is re-run in 09.)

## Design notes (modules, commands/events, data model + migration, UI components)

**Why DB-level encryption.** TAURI_MVP's choice stands (`TAURI_MVP.md:82-83`): one mechanism covers text, titles, segments and audio, with no per-field gaps (Electron left timestamp/model/prompt id in clear, `sessions.js:218-259`) and no separate audio crypto code, nonce/format design, or key file. Audio as BLOBs also keeps the existing crash-safe chunk writing.

**Modules**
- `src-tauri/src/store/key.rs` (new): `trait KeyStore`, `KeychainKeyStore` (macOS), `MemoryKeyStore` (tests/bench), the R5 decision function `plan_open(DbState, KeyState) -> Plan`, hex key helper. Key buffers are overwritten after use on a best-effort basis (note the limit: Rust may have copied them; do not claim zeroization).
- `src-tauri/src/store/mod.rs`: `Store::open(path, &dyn KeyStore) -> Result<Store>`. Pragma order: `key` → verify (`SELECT count(*) FROM sqlite_master`) → `cipher_version` check → `journal_mode=WAL` → `foreign_keys` → `secure_delete` → `busy_timeout` → migrate → recover. New `StoreError::Locked(LockReason)` with `LockReason::{KeychainDenied, KeyMissing, WrongKey, MigrationFailed, NotSqlcipherBuild, KeychainWriteFailed}`.
- `src-tauri/src/store/plaintext_migration.rs` (new): R8/R9 (export, verify, swap, scrub). Pure file-state classifier so AS10 can inject crash points.
- `src-tauri/src/store/migrations.rs`: `audio_kept` migration, appended as the next free version at implementation time (v4 if 01 lands first, as planned): `ALTER TABLE sessions ADD COLUMN audio_kept INTEGER NOT NULL DEFAULT 0;` plus `CREATE INDEX sessions_audio_kept ON sessions(audio_kept, created_at) WHERE audio_kept = 1;`. Never edit v1-v3. (The encrypted-export migration runs **before** the normal migrations, never as a numbered migration.)
- `src-tauri/src/store/sessions.rs`: `save_transcript(id, transcript, duration_ms, keep_audio)` (the one transaction); new `sweep_expired_audio(now_ms) -> usize`; `session_audio_mixed(id) -> Result<Vec<i16>>` (reuse `audio_samples`, `:536`); `Session` gains `has_audio: bool` and `audio_expires_at: Option<i64>`; `SessionListItem` gains `has_audio: bool` (so history can show an icon later without a refetch).
- `src-tauri/src/store/settings.rs`: `keep_audio` key; `RecordingSettings.keep_audio`.
- `src-tauri/src/jobs/session.rs:607`: pass the live setting into `save_transcript`.
- `src-tauri/src/jobs/retention.rs` (new, small): hourly tick on a thread with injectable clock (same style as the queue); `lib.rs` spawns it after `Store::open`.
- `src-tauri/src/commands.rs` + `lib.rs` handler list: `request_session_audio`, `locked_state` (query for the startup screen) and `start_fresh(confirmation: String)`. Register new types in `export_ipc_types` (`commands.rs:282-322`) so ts-rs regenerates `src/lib/types/` (`Session.ts`, `SessionListItem.ts`, `RecordingSettings.ts`, new `AudioError.ts`, `LockReason.ts`).
- `src-tauri/src/lib.rs:70-74`: on `Store::open` error of kind `Locked`, do **not** return the error (that would crash setup); manage a `LockedState` instead and let the webview render the blocking screen. Commands taking `State<Arc<Store>>` need a guard: simplest is to manage `Arc<Store>` only when unlocked, and make the frontend call `locked_state` first and render only the lock screen when set. Verify no command panics on missing managed state (Tauri returns an error for missing state; a `cargo test` over the handler list is not practical, so cover with manual AS7).
- `src-tauri/tauri.conf.json:21`: CSP add `; media-src blob:`.
- `src/lib/api.ts`: `requestSessionAudio(id): Promise<ArrayBuffer>`, `lockedState()`, `startFresh(confirmation)`; extend `api.test.ts`.
- `src/lib/components/Settings.svelte`: "Audio recording" section (R13).
- `src/lib/components/SessionAudio.svelte` (new): the player; owns blob lifetime. No session detail view exists until spec 09 (`App.svelte:13` has only `record`/`settings`), so mount it, with the R19 retention line, on the current-session result in `RecordView`/`SummaryPanel` above the transcript. Spec 09 re-mounts both in session detail.
- `src/lib/components/LockedScreen.svelte` (new) and a one-time notice component for the migration result.
- `release_session_audio`: Spec author's call is **not** to add it. A command that does nothing is dead surface; the frontend's `revokeObjectURL` is the real release. Electron needed the pair only because of the temp file. (R20 now says "not added". Neither 05 nor 08 needs a window-close hook for it.)

**Dependencies change in `Cargo.toml`.** `rusqlite` feature `bundled` → `bundled-sqlcipher-vendored-openssl` (see below). Cargo does not allow both `bundled` and a SQLCipher feature to coexist sanely, so `bundled` must go.

**Security claims (what may be said, in docs/UI/marketing)**
May say: "Your sessions are encrypted on this Mac. The key is held in your macOS Keychain, not in the file." "Recordings are off by default, and when on, are encrypted and deleted after 7 days."
Must not say or imply:
- HIPAA/GDPR "compliance", "military-grade", "unhackable", or that Scribular holds no data (it holds it, encrypted).
- Protection from malware or another process running as the logged-in user. While the app is unlocked, the key is retrievable by the user's own session (the Keychain ACL is per app, but a same-user attacker can attach to or read the running process) and decrypted text lives in app and webview memory.
- Protection of memory pages swapped to disk, hibernation images, Time Machine copies of other files, or the clipboard after the user copies a summary. FileVault is the answer for swap/hibernation; recommend it in onboarding copy (spec 07).
- Secure erasure on SSD/APFS. Deleted audio is zeroed in the DB (`secure_delete`) and sits encrypted in free pages, but we cannot promise the physical blocks are gone; encryption is what protects them.
- Backups: because the key is `ThisDeviceOnly`, a Time Machine restore onto a different Mac yields an unreadable DB. **State this plainly** in docs/FAQ ("Sessions cannot be moved to another Mac"). Same-Mac restore after reinstall works while the Keychain item survives.
- That the 7-day window is exact to the minute (sweep is hourly while open, plus launch and playback checks; if the Mac sleeps, expiry happens at the next tick or launch, before the audio is playable because of the lazy check).

**Residual risks to carry into REVIEW.md**
1. The webview holds decoded audio/transcripts in memory and (WebKit) may keep a blob in its process until revoked; the revoke path matters.
2. The Rust process memory holds the hex key string briefly during `PRAGMA key`.
3. `cipher_memory_security` (SQLCipher default on) helps, but is not a guarantee.
4. Prototype plaintext remnants as in R9.
5. Wrong feature flag = plaintext DB: mitigated by R4/AS3 and by AS1 run on release builds in CI.

## New dependencies (candidates + review needed)
First implementation step: write `plans/DEPENDENCY_REVIEW_ENCRYPTION.md` (version rule, owners, licence, advisories, exact pins, trial-resolve lockfile diff) and get Patrick's approval before any `Cargo.toml` edit, following `DEPENDENCY_REVIEW_M5A.md`.

| Candidate | Why | In-house alternative |
|---|---|---|
| `rusqlite` feature change (`bundled` → `bundled-sqlcipher-vendored-openssl`) pulling `openssl-src`/`openssl-sys` into the lockfile | SQLCipher needs a crypto provider. Plain `bundled-sqlcipher` links to a system OpenSSL, which end users do not have (and Homebrew's would be a broken dynamic link in a shipped app). The `vendored-openssl` variant builds OpenSSL into the binary; needs `perl`/`make` on the CI runner (macOS runners have them) and lengthens builds. **Verify in the trial build** that `otool -L` of the release binary lists no `libcrypto`/`libssl` dylib outside the system. If it does, stop and re-plan. | None. Writing SQLCipher or an encrypted-pager VFS in-house is not realistic and not safe. |
| `security-framework` (recommended, macOS only; pin exact) | Direct Keychain get/set of one generic password. It is already what `keyring` uses on macOS. Fewer layers. | Raw `SecItem*` FFI via `extern "C"` (about 80 lines, unsafe). Not recommended; more risk than a reviewed crate. |
| `keyring` (alternative named in `TAURI_MVP.md:272`) | Cross-platform wrapper; unneeded for a Mac-only product. | Same as above. |
| `getrandom` (or `rand`), likely already transitive; check `Cargo.lock` | 32 CSPRNG bytes for the key | `std` has no CSPRNG; reading `/dev/urandom` via `std::fs` is an acceptable 5-line in-house fallback on macOS if no existing crate is in the tree. |
| `zeroize` (optional) | Best-effort key-buffer scrubbing | A manual `write_volatile` loop. Skip unless review wants it. |

No npm packages. No Tauri plugins. (`tauri-plugin-stronghold` and `tauri-plugin-store` are not used.)

## Out of scope
- Electron-to-Tauri data/settings migration.
- Diarization audio use (spec 02 uses the same chunks while the transcript is built; it must not make retention longer. If spec 02 re-reads audio after `save_transcript`, then `save_transcript` can no longer delete it. Flag in "Depends on").
- Licence/trial data storage and any Keychain use beyond the DB key (spec 04 may add its own Keychain item; reuse `KeyStore` conventions, not the same account).
- Crash-recovery transcription UX (spec 09). This spec only guarantees interrupted audio exists until day 7.
- Diagnostics redaction policy beyond the AS11 canary test (spec 06).
- Audio export, trimming, waveform, speed control, per-session "delete recording".
- Auto-lock, passcode, Touch ID.

## Depends on / interacts with (other parity specs by slug)
- **09-session-management-recovery**: owns clear-all, delete UX, recovery of interrupted recordings. Needs R16 (7-day window for interrupted audio) and R18 (`VACUUM` after clear-all). Both specs touch `save_transcript` and `recover_interrupted`; sequence 01 first, or co-ordinate.
- **02-multi-speaker-diarization**: reads the PCM chunks; it must finish before `save_transcript` deletes them, or move deletion to the end of diarization. Hand the `keep_audio` evaluation point (R13a) to whichever step ends the pipeline.
- **06-diagnostics-telemetry**: logs/diagnostics must pass the AS11 canary; "copy diagnostics" may report `cipher_version` and lock reason, never the key or paths with session ids in clear text beyond what is needed.
- **07-onboarding-samples**: first-run copy about FileVault and "encrypted on this Mac"; sample sessions are written via the encrypted store. First-run screen must not run before the lock check.
- **05-auto-update**: updates must be signed with the same identity so the Keychain item stays accessible (AS8). A schema-too-new rollback shows `SchemaTooNew` (already exists).
- **04-licensing-trial**: separate secrets, same `KeyStore` trait style.
- **08-app-shell-tray-notifications**: the tray must not offer recording while the store is Locked.
- **03-note-styles-model-choice**: also edits `RecordingSettings`/`settings` and `commands.rs` type exports; expect merge conflicts in `settings.rs` and the ts-rs export list.

## Coordination notes (gap-check 2026-11-04)
- **Keychain crate is owned here.** 01's dependency review covers `security-framework` for all Keychain use. Spec 04 reuses `KeyStore`/`KeychainKeyStore` with account `license` and needs no separate Keychain review.
- **Bundle identifier = Keychain service + data dir.** `tauri.conf.json:5` (`com.app.scribular.tauri`) must be frozen before 01 ships to testers; changing it later orphans the key (Locked screen). The identity decision lives in spec 05 (recommended: keep it).
- **Interrupted audio:** 01 guarantees retention until day 7 only. Recovery UX is spec 09. Electron has no recovery, so there is no parity gap to fill here.
- **Diarization (02):** runs inside finalize, before `save_transcript` applies the `keep_audio` deletion (R13a). 02 must keep that order; retention must not grow.
- **Locked state:** spec 07's FirstRun runs after the lock check; spec 08's tray hides Start while Locked; spec 04's licence read happens after unlock.
- **Native libs:** the release verify step (`.github/workflows/release-mac.yml:40-50`) must also assert no non-system `libcrypto`/`libssl` in the release binary (the `otool -L` check above). Add it in PR2.

## Decisions for Patrick (with recommended default)
1. **Fixed 7-day retention or user-selectable?** Default: fixed 7 days (Electron parity; one constant). Changes UI and the sweep input.
2. **Key is this-device-only (Time Machine restore on a new Mac cannot open the DB) or synced via iCloud Keychain?** Default: this-device-only (smaller attack surface, no cloud copy of the key). Cost: therapists who change Macs lose history unless a future export exists. Must be in the docs. This is the one choice that can cost a user their data, so confirm it.
3. **When the user turns "Save session audio" off, delete already-kept recordings?** Default: no (Electron parity; they expire on schedule). Alternative: delete them immediately with a confirm. For a therapy product, immediate delete-on-disable may be what privacy-minded users expect.
4. **Keychain failure behaviour:** default is the Locked screen with "Start fresh" (data stays on disk, encrypted, renamed). The alternative, deleting the unreadable DB, is rejected. Confirm "Start fresh" is acceptable product behaviour.
5. **Keep migrating testers' prototype DBs?** Default: migrate (cheap via `sqlcipher_export`, avoids data loss, plan says prototype data is test/demo only). The alternative is wiping them with a prompt, which is less code and less risk of leaving plaintext behind but loses tester sessions.
6. **Add a per-session "Delete recording now" button?** Default: no (Electron has none; expiry or session delete only). It is a small addition (one command, one button) if therapists want to delete audio right after checking.
7. **Dependency approval** for the SQLCipher/vendored-OpenSSL build change and `security-framework` (see above). Default: approve `security-framework`; approve the rusqlite feature only if the trial build shows a self-contained binary.

## Size (S/M/L, rough days) and suggested PR split if > ~1 PR
**L, about 8-10 working days**, split into 4 PRs:
1. **PR1 (0.5-1 d)**: `DEPENDENCY_REVIEW_ENCRYPTION.md` only. Needs Patrick's approval before PR2.
2. **PR2 (3-4 d)**: encryption core: `key.rs`, `Store::open(path, &KeyStore)`, `cipher_version` guard, decision table, Locked state + `LockedScreen`, AS1-AS8, AS11. No audio behaviour change. Bench/test callers moved to `MemoryKeyStore`.
3. **PR3 (1.5-2 d)**: prototype-DB migration (R8-R10) with crash-injection tests, one-time notice, AS9, AS10, AS10b.
4. **PR4 (3 d)**: keep-audio setting, `audio_kept` migration, sweep + hourly tick, expiry display, playback command + `SessionAudio.svelte`, CSP, AS12-AS22.

Pre-existing weaknesses in Electron that this spec deliberately does not copy: fail-open encryption (`sessions.js:50`), silent key regeneration (`audioEncryption.js:32-35`), plaintext temp WAVs (`audio.js:565`, `registerIpcHandlers.js:224`), launch-only expiry (`index.js:436`), plaintext metadata (`sessions.js:218-259`), silent player failure (`session.js:447-449`).
