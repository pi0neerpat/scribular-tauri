# Theme PR1: appearance state

Shipped in PR #49 (PR1 of the theme work). Spec: `plans/2026-11-16-theme-spec.md` (PR1 covers AS1-AS5 only). This page covers the persisted appearance state, its IPC contract, and preset resolution. PR2 added the Appearance pane controls and the window background picture (see Background pictures below).

## What it does

- **Stores one appearance value.** The selection (Default, Scribular V1, Sage, or My Theme), a global mode (Automatic, Light, Dark), and an optional customized My Theme are saved together as one JSON value.
- **Falls back safely.** A fresh store, a store with no appearance row, a corrupt value, or an invalid value loads as Default with Automatic mode and no My Theme.
- **Rejects bad saves.** `set_appearance` refuses invalid input and leaves the previous saved value untouched.
- **Resolves to colors.** The frontend turns the saved state plus the Mac's light/dark setting and accent color into effective tokens (dark flag, accent, tint, text, record color, background).
- **Syncs across windows.** Every successful save emits `appearance://changed` with the saved value.

## How it works

- **Data model.** Rust types in `src-tauri/src/store/appearance.rs`, exported to `src/lib/types/` by ts-rs (`Appearance.ts`, `MyTheme.ts`, `Preset.ts`, `Selection.ts`, `ThemeValues.ts`, `Tint.ts`, `Mode.ts`, `Accent.ts`, `Background.ts`). All serialize camelCase.
  - `Appearance`: `version` (must be 1), `selected` (`default | scribularV1 | sage | myTheme`), `mode`, `myTheme` (nullable).
  - `MyTheme`: `origin` (a `Preset`: `default | scribularV1 | sage`), `copiedMode` (nullable, defaults to null when absent), `values`.
  - `ThemeValues`: `mode`, `accent`, `tint` (`none | warm | cool | sage | rose`), `background`, `blur`, `brightness`, `starsMotion`.
  - `Accent`: `system | blue | purple | pink | red | orange | yellow | green | graphite | {custom: "#rrggbb"}`.
  - `Background`: `none | stars | dawn | seaGlass | linen | {owned: "<id>"}`.
- **Storage.** Saved under the `appearance_v1` key in the `settings` table of the encrypted `scribular.db`, via `Store::appearance` and `Store::set_appearance`. A save runs in one transaction.
- **Validation (load and save).** The value is valid only if:
  - `version` is 1;
  - `selected` is not `myTheme` while `myTheme` is null;
  - a custom accent is exactly `#` plus six hex digits;
  - an owned background id is 1-64 characters of ASCII letters, digits, or `-`;
  - `blur` and `brightness` are at most 100.
  
  On load, unparsable JSON or an invalid value returns the default. On save, an invalid value returns `StoreError::InvalidAppearance` ("Choose valid appearance settings.") and nothing is written.
- **IPC.** `src-tauri/src/commands.rs` registers `get_appearance` (returns the stored or default value) and `set_appearance` (validates, saves, emits `appearance://changed`, returns the saved value). Wrappers `getAppearance`, `setAppearance`, and `onAppearanceChanged` are in `src/lib/api.ts`.
- **Preset resolution** (`src/lib/appearance.ts`, `presetValues` and `resolveAppearance`):
  - Default: Automatic mode, system accent, no tint, no background.
  - Scribular V1: always dark (the global mode is ignored and `modeEditable` is false), blue accent resolved to `#057dcd`, no tint, Stars background with motion; dark tint `#000000`, text `#e8eef1`.
  - Sage: follows the global mode, green accent resolved to `#59a682` (dark) or `#467c63` (light), Sage tint, no background.
  - My Theme: uses its own `values`, including its own mode. It keeps its `origin` palette for the V1 and Sage accent, tint, and text tweaks above.
  - Automatic mode follows the Mac's dark setting. The system accent comes from the CSS `AccentColor`. Tints change only the surface color; text and the record color (`#e53935`) do not change.
- **My Theme helpers** (`src/lib/appearance.ts`):
  - `selectPreset` switches the selection and never discards `myTheme`. Selecting `myTheme` with none saved is a no-op.
  - `customize(state, preset, replaceConfirmed)` copies a preset into My Theme. If My Theme already exists and replacement is not confirmed, state is unchanged.
  - `copiedMode` is the mode copied at customize time: `dark` for Scribular V1, otherwise the global mode.
  - `resetMyTheme` restores the origin preset's values, keeps `origin`, and keeps the mode from `copiedMode`. A legacy My Theme with `copiedMode` null keeps its current `values.mode`.
- **Frontend store** (`src/lib/appearance-store.ts`): `appearance` and `effectiveAppearance` Svelte stores, plus `appearanceError`.
  - `startAppearance` subscribes to the event first, then loads a snapshot. A snapshot that arrives after an event is ignored. It also watches the Mac's light/dark and accent (`watchSystemAppearance`, polled every second because WebKit has no accent event).
  - `saveAppearance` takes a value or an updater `(latest) => next`; the pane passes updaters so an edit queued behind a picture import builds on the imported picture. It chains saves on one promise, so overlapping saves run one at a time in order. A response or failure from a save that was overtaken by a newer `appearance://changed` event is dropped, so a stale result cannot overwrite newer state or show an obsolete error.

## Config and data

- Key `appearance_v1` in the `settings` table of `scribular.db` (app data directory). Value is the `Appearance` JSON. Absent means Default.
- The older animation setting (`show_background_animation`) is not read by this state.
- Event: `appearance://changed`. Payload is the saved `Appearance`.
- Types in `src/lib/types/` are generated; do not edit them by hand.

## How to test

- Frontend: `npm test`. Appearance files: `src/lib/appearance.test.ts` (built-in resolution, My Theme preserve/replace/reset, accents and tints) and `src/lib/appearance-store.test.ts` (serialized saves, stale responses and errors).
- Type and Svelte check: `npm run check`.
- Rust: `cargo test --lib` from `src-tauri`. Tests in `src-tauri/src/store/appearance.rs` cover fallback for fresh, legacy, and unknown values, loading a My Theme saved before `copiedMode` existed, and atomic saves that reject invalid values.
- Real app (uitest): release builds show "Trial ended" on this Mac, so build a debug bundle with `npm run tauri -- build --debug --bundles app` and launch with `SCRIBULAR_SKIP_LICENSE=1`. Other build and run steps are in `CLAUDE.md` and `tools/uitest/README.md`. `App.svelte` and `Settings.svelte` call `startAppearance`. The Appearance pane edits the state, and `App.svelte` mounts `ThemeBackground` from `effectiveAppearance`.

## Known limits

- Minimal controls only: the Appearance pane (`AppearancePane.svelte`) has theme radios, Customize/Reset, and the background picture controls. Accent, tint and mode controls, and applying the resolved colors, come in PR3.

## Background pictures (PR2)

- Bundled pictures: `src/assets/backgrounds/{dawn,sea-glass,linen}.jpg`, resolved by `backgroundSource` in `src/lib/appearance.ts`. Stars draws on a canvas; None shows the solid background.
- Import: `choose_background_picture` (native Open panel) and `import_background_picture` (Settings drop) in `src-tauri/src/commands.rs`, backed by `src-tauri/src/backgrounds.rs`. Scribular sniffs JPEG, PNG, HEIC or WebP content, decodes it with macOS `sips`, downsizes it to cover the largest reported width and the largest reported height across connected displays, taken independently (never upscales), and writes an owned copy to `<db dir>/Backgrounds/<id>.jpg|png` (folder mode 0700). Import needs a My Theme and selects it. Undecodable or other files show the unsupported-picture error and leave the theme unchanged.
- Cleanup: after every appearance save or import, `Backgrounds::sweep` removes owned copies that the saved My Theme no longer references. Sessions never reference pictures, so deleting sessions leaves the selected picture alone. `set_appearance` refuses to select My Theme when its owned picture file is missing; built-ins stay selectable and My Theme keeps the reference. Unexpected store failures return "The settings could not be written." instead of SQLite or JSON details.
- Loading: the custom `scribular-bg://localhost/<id>` scheme serves only valid ids from the owned folder (`Cache-Control: no-store`, 404 otherwise). The CSP directive is `img-src 'self' data: scribular-bg:`; PR2 added only `scribular-bg:`, and the Tauri asset protocol stays off. The handler reads the file off the webview thread. A picture that fails to load falls back to the solid background; for an owned picture the Appearance pane also shows "The background picture is missing or damaged. Choose it again or reset My Theme."
- Stars motion: `starsState` in `src/lib/components/background-animation.ts`. Only Stars mounts the canvas; its Animated motion switch, a hidden window and Reduce Motion keep it still, and recording speeds it up. Choosing Stars turns motion on.
- Tests: `src-tauri/src/backgrounds.rs` (AS6 import, AS7 cleanup, failed commit, scheme), `src/lib/components/background-animation.test.ts` and `theme-background.svelte.test.ts` (AS8), `src/lib/appearance.test.ts` (sources).
