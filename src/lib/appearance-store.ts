import { writable } from 'svelte/store';
import { chooseBackgroundPicture, getAppearance, importBackgroundPicture, onAppearanceChanged, setAppearance } from './api';
import { defaultAppearance, resolveAppearance, watchSystemAppearance } from './appearance';
import type { Appearance } from './types/Appearance';

export const appearance = writable<Appearance>(defaultAppearance);
export const effectiveAppearance = writable(resolveAppearance(defaultAppearance, false, '#007aff'));
export const appearanceError = writable<string | null>(null);

let current = defaultAppearance;
let systemDark = false;
let systemAccent = '#007aff';
let changeVersion = 0;
let saves: Promise<void> = Promise.resolve();
const apply = (next: Appearance) => {
  current = next;
  appearance.set(next);
  effectiveAppearance.set(resolveAppearance(next, systemDark, systemAccent));
};

// Saves run one at a time in order. A result or failure from a save that a
// newer appearance://changed event overtook is dropped.
function queueSave(run: () => Promise<Appearance>, failure: (error: unknown) => string): Promise<void> {
  saves = saves.then(async () => {
    const version = changeVersion;
    appearanceError.set(null);
    try {
      const saved = await run();
      if (version === changeVersion) apply(saved);
    } catch (error) {
      if (version === changeVersion) appearanceError.set(failure(error));
    }
  });
  return saves;
}

// An updater is applied to the latest appearance when its turn comes, so an
// edit queued behind a picture import keeps the new picture.
export function saveAppearance(next: Appearance | ((latest: Appearance) => Appearance)): Promise<void> {
  return queueSave(
    () => setAppearance(typeof next === 'function' ? next(current) : next),
    (error) => `Could not save Appearance. Try again. ${String(error)}`,
  );
}

// Theme R6/R7: Rust copies the picture and commits it to My Theme. A failure
// or cancel leaves the current picture; Rust errors are safe user text.
const savePicture = (run: () => Promise<Appearance>) => queueSave(run, String);

export function chooseBackground(): Promise<void> { return savePicture(chooseBackgroundPicture); }
export function dropBackground(path: string): Promise<void> { return savePicture(() => importBackgroundPicture(path)); }

// Theme R12: shown when the selected owned picture cannot be displayed.
export const PICTURE_UNAVAILABLE = 'The background picture is missing or damaged. Choose it again or reset My Theme.';
export function reportPictureUnavailable(): void { appearanceError.set(PICTURE_UNAVAILABLE); }

export async function startAppearance(): Promise<() => void> {
  const stopSystem = watchSystemAppearance((dark, accent) => {
    systemDark = dark;
    systemAccent = accent;
    effectiveAppearance.set(resolveAppearance(current, dark, accent));
  });
  let changed = false;
  try {
    const stopEvents = await onAppearanceChanged((next) => { changed = true; changeVersion++; apply(next); appearanceError.set(null); });
    try { const snapshot = await getAppearance(); if (!changed && snapshot) apply(snapshot); }
    catch (error) { appearanceError.set(`Could not load Appearance. ${String(error)}`); }
    return () => { stopEvents(); stopSystem(); };
  } catch (error) {
    stopSystem();
    appearanceError.set(`Could not watch Appearance. ${String(error)}`);
    return () => {};
  }
}
