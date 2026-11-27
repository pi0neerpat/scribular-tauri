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

export function saveAppearance(next: Appearance): Promise<void> {
  saves = saves.then(async () => {
    const version = changeVersion;
    appearanceError.set(null);
    try {
      const saved = await setAppearance(next);
      if (version === changeVersion) apply(saved);
    } catch (error) {
      if (version === changeVersion) appearanceError.set(`Could not save Appearance. Try again. ${String(error)}`);
    }
  });
  return saves;
}

// Theme R6/R7: Rust copies the picture and commits it to My Theme. A failure
// or cancel leaves the current picture; Rust errors are safe user text.
function savePicture(run: () => Promise<Appearance>): Promise<void> {
  saves = saves.then(async () => {
    const version = changeVersion;
    appearanceError.set(null);
    try {
      const saved = await run();
      if (version === changeVersion) apply(saved);
    } catch (error) {
      appearanceError.set(String(error));
    }
  });
  return saves;
}

export function chooseBackground(): Promise<void> { return savePicture(chooseBackgroundPicture); }
export function dropBackground(path: string): Promise<void> { return savePicture(() => importBackgroundPicture(path)); }

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
