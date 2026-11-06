# Tauri releases

`release-mac.yml` builds the Apple signed and notarized arm64 DMG and the Tauri signed updater archive. Run it from `main` with `publish=false` for a verification build. Use `publish=true` only when the DMG and extracted updater bundle pass all signing checks. The workflow creates a draft in `pi0neerpat/scribular-tauri-releases`, uploads the DMG, archive and signature, uploads `latest.json` last, then publishes and commits the version bump and tag to `main`.

## One-time updater setup

The updater key is independent from the Apple Developer ID certificate. Patrick generates it locally and keeps an offline backup of the private key; losing it strands all installed copies because v1 has no key rotation. Never commit the private key or put it in logs.

```sh
npx tauri signer generate -w ~/.tauri/scribular-tauri-updater.key
gh secret set TAURI_SIGNING_PRIVATE_KEY -R pi0neerpat/scribular-tauri < ~/.tauri/scribular-tauri-updater.key
gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD -R pi0neerpat/scribular-tauri
gh secret set RELEASES_REPO_TOKEN -R pi0neerpat/scribular-tauri
```

The token needs Contents read/write access to `pi0neerpat/scribular-tauri-releases` only. Add the generated public key to `plugins.updater.pubkey` in `src-tauri/tauri.conf.json`; the committed placeholder intentionally cannot verify releases. Keep the Tauri bundle identifier and product name unchanged.

## Publishing and rollback

The feed is public and separate from Electron's `pi0neerpat/scribular-releases`. Never publish Tauri assets to the Electron feed. The first updater-enabled build is the update baseline; older 0.1.0 installs need one manual reinstall.

If a published release is bad, switch that release back to draft in `pi0neerpat/scribular-tauri-releases`. The `/releases/latest/download/latest.json` URL then resolves to the previous published release. Clients already updated do not downgrade; fix forward with a higher version. Never delete the private key to stop updates.

Update installation replaces only the `.app` bundle; data in `app_data_dir` remains in place. SQLite migrations are forward-only, so a rollback binary may encounter a newer database schema. Every release with a database migration must call it out in its release notes and record the compatibility risk here or in its linked migration notes.
