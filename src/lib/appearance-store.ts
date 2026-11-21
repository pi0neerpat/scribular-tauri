import { writable } from 'svelte/store';
import { getAppearance, onAppearanceChanged, setAppearance } from './api';
import { defaultAppearance, resolveAppearance, watchSystemAppearance } from './appearance';
import type { Appearance } from './types/Appearance';

export const appearance = writable<Appearance>(defaultAppearance);
export const effectiveAppearance = writable(resolveAppearance(defaultAppearance, false, '#007aff'));
export const appearanceError = writable<string | null>(null);

let current = defaultAppearance;
let systemDark = false;
let systemAccent = '#007aff';
const apply = (next: Appearance) => {
  current = next;
  appearance.set(next);
  effectiveAppearance.set(resolveAppearance(next, systemDark, systemAccent));
};

export async function saveAppearance(next: Appearance): Promise<void> {
  appearanceError.set(null);
  try { apply(await setAppearance(next)); }
  catch (error) { appearanceError.set(`Could not save Appearance. Try again. ${String(error)}`); }
}

export async function startAppearance(): Promise<() => void> {
  const stopSystem = watchSystemAppearance((dark, accent) => {
    systemDark = dark;
    systemAccent = accent;
    effectiveAppearance.set(resolveAppearance(current, dark, accent));
  });
  let changed = false;
  try {
    const stopEvents = await onAppearanceChanged((next) => { changed = true; apply(next); });
    try { const snapshot = await getAppearance(); if (!changed && snapshot) apply(snapshot); }
    catch (error) { appearanceError.set(`Could not load Appearance. ${String(error)}`); }
    return () => { stopEvents(); stopSystem(); };
  } catch (error) {
    stopSystem();
    appearanceError.set(`Could not watch Appearance. ${String(error)}`);
    return () => {};
  }
}
