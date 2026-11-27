# Theme background pictures and Stars motion

Shipped in PR #69 (PR2 of the theme work). Spec: `plans/2026-11-16-theme-spec.md` (PR2 covers AS6-AS8 and AS11). Persisted state and presets are in [theme-appearance-state.md](theme-appearance-state.md).

## What it does

- **Window picture.** My Theme can show a bundled picture (dawn, sea glass, linen), Stars, none, or a picture the user imports from disk.
- **Import.** The Settings Appearance pane offers a native Open panel (`choose_background_picture`) and drag-and-drop (`import_background_picture`). Scribular stores its own downsized copy; the original is never read again and its path never reaches the webview.
- **Safe failures.** Cancel does nothing. A corrupt or unsupported file shows the unsupported-picture error and keeps the prior selection with no partial file. A missing or damaged owned picture falls back to the solid background.
- **Cleanup.** Owned copies that the saved My Theme no longer references are deleted. Deleting sessions never touches theme pictures.
- **Stars motion.** Only the Stars background mounts the canvas. It animates only when the Animated switch is on, the window is visible, and Reduce Motion is off. Recording speeds it up.

## How it works

- **Import** (`src-tauri/src/backgrounds.rs`). Content is sniffed (not trusted by extension) as JPEG, PNG, HEIC or WebP. Limits: 100 MB file size, 20,000 px per side. `/usr/bin/sips` decodes and downsizes the picture to cover the largest display width and height (taken independently, never upscaled; fallback 5120x2880). The copy is written to `<db dir>/Backgrounds/<id>.jpg|png`. Import requires a My Theme and selects it (otherwise "Customize a theme before choosing a picture.").
- **Sweep.** `Backgrounds::sweep` runs after every appearance save and import and removes unreferenced files. `set_appearance` refuses to select My Theme when its owned file is missing.
- **Loading.** `ThemeBackground.svelte` renders bundled files or `scribular-bg://localhost/<id>` in an `<img>`; blur and brightness are applied as a CSS filter on the picture only. `onerror` falls back to the solid background and, for owned pictures, shows "The background picture is missing or damaged. Choose it again or reset My Theme."
- **Stars.** `starsState` in `src/lib/components/background-animation.ts` decides running/speed; `BackgroundAnimation.svelte` draws it.

## Config and data

- **Backgrounds folder.** `<db dir>/Backgrounds/`, created with mode 0700. Files are named by generated id (ASCII letters, digits, `-`, 1-64 chars, same rule as the `{owned: "<id>"}` background).
- **`scribular-bg` protocol.** Registered in `src-tauri/src/lib.rs`; `backgrounds::respond` serves only valid ids from the owned folder, with `Cache-Control: no-store` and 404 otherwise. File reads happen off the webview thread.
- **CSP** (`src-tauri/tauri.conf.json`): `default-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: scribular-bg:; media-src blob:`. PR2 added only `scribular-bg:` to `img-src`. The Tauri asset protocol stays off, so there is no filesystem-wide asset route.
- **Bundled assets.** `src/assets/backgrounds/{dawn,sea-glass,linen}.jpg`, resolved by `backgroundSource` in `src/lib/appearance.ts`.

## How to test

- Rust (AS6 import, AS7 cleanup, failed commit, scheme): `cargo test --lib backgrounds` from `src-tauri/`. Fixtures: `src-tauri/tests/fixtures/backgrounds/picture.{jpg,png,heic,webp}`. Requires macOS `sips`.
- Frontend (AS8 and picture fallback): `npm test`; key files are `src/lib/components/background-animation.test.ts`, `theme-background.svelte.test.ts` and `src/lib/appearance.test.ts`.
- uitest (idea, not yet written): an `as11` scenario that opens Settings > Appearance, picks My Theme, chooses a fixture picture through the Open panel, asserts the owned picture shows, then chooses a corrupt file and asserts the error text with the prior picture kept. Follow `tools/uitest/README.md` and the CLAUDE.md uitest rules (debug build, `SCRIBULAR_SKIP_LICENSE=1`).
- Manual AS11 inspection: list the Backgrounds folder (mode 0700, only referenced files), and confirm the CSP and the `scribular-bg` route in the debug build. Record filenames and error text, not picture contents.

## Known limits

- Drag-and-drop is not driven by uitest (no synthetic drop onto the webview); drop is covered only by unit tests and manual checks.
- AS9 (manual real-app check across Default, V1, Sage, My Theme, bright/busy pictures, Light/Dark and every window) is deferred to PR3, along with the theme tokens across windows.
- Import depends on macOS `sips`; HEIC decoding is as good as the OS provides.
