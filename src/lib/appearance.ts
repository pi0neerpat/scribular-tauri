import type { Accent } from './types/Accent';
import type { Appearance } from './types/Appearance';
import type { Mode } from './types/Mode';
import type { Preset } from './types/Preset';
import type { Selection } from './types/Selection';
import type { ThemeValues } from './types/ThemeValues';
import type { Tint } from './types/Tint';

export const defaultAppearance: Appearance = { version: 1, selected: 'default', mode: 'automatic', myTheme: null };

export function presetValues(preset: Preset): ThemeValues {
  switch (preset) {
    case 'scribularV1': return { mode: 'dark', accent: 'blue', tint: 'none', background: 'stars', blur: 0, brightness: 100, starsMotion: true };
    case 'sage': return { mode: 'automatic', accent: 'green', tint: 'sage', background: 'none', blur: 0, brightness: 100, starsMotion: false };
    default: return { mode: 'automatic', accent: 'system', tint: 'none', background: 'none', blur: 0, brightness: 100, starsMotion: false };
  }
}

export function selectPreset(state: Appearance, selected: Selection): Appearance {
  if (selected === 'myTheme' && !state.myTheme) return state;
  return { ...state, selected };
}

export function customize(state: Appearance, preset: Preset, replaceConfirmed = false): Appearance {
  if (state.myTheme && !replaceConfirmed) return state;
  const copiedMode = preset === 'scribularV1' ? 'dark' : state.mode;
  return { ...state, selected: 'myTheme', myTheme: { origin: preset, copiedMode, values: { ...presetValues(preset), mode: copiedMode } } };
}

export function resetMyTheme(state: Appearance): Appearance {
  if (!state.myTheme) return state;
  const { origin, copiedMode, values } = state.myTheme;
  return { ...state, selected: 'myTheme', myTheme: { origin, copiedMode, values: { ...presetValues(origin), mode: copiedMode ?? values.mode } } };
}

const accents: Record<Exclude<Accent, { custom: string }>, string> = {
  system: '', blue: '#007aff', purple: '#af52de', pink: '#ff2d55', red: '#ff3b30',
  orange: '#ff9500', yellow: '#ffcc00', green: '#34a853', graphite: '#8e8e93',
};
const tints: Record<Exclude<Tint, 'none'>, { light: string; dark: string }> = {
  warm: { light: '#fff2e6', dark: '#33271f' }, cool: { light: '#edf4ff', dark: '#1d2938' },
  sage: { light: '#fcfbf8', dark: '#110f0d' }, rose: { light: '#fff0f3', dark: '#38252d' },
};

export function resolveAppearance(state: Appearance, systemDark: boolean, systemAccent: string) {
  const preset = state.selected === 'myTheme' ? null : state.selected;
  const palette = preset ?? state.myTheme?.origin;
  const values = preset ? presetValues(preset) : state.myTheme?.values ?? presetValues('default');
  const mode: Mode = preset === 'scribularV1' ? 'dark' : preset ? state.mode : values.mode;
  const dark = mode === 'dark' || (mode === 'automatic' && systemDark);
  const accent = palette === 'scribularV1' && values.accent === 'blue' ? '#057dcd'
    : palette === 'sage' && values.accent === 'green' ? (dark ? '#59a682' : '#467c63')
    : typeof values.accent === 'string' ? accents[values.accent] || systemAccent : values.accent.custom;
  const tint = values.tint === 'none'
    ? palette === 'scribularV1' && dark ? '#000000' : (dark ? '#1c1c1e' : '#ffffff')
    : tints[values.tint][dark ? 'dark' : 'light'];
  return {
    dark, modeEditable: preset !== 'scribularV1', accent, tint,
    text: palette === 'scribularV1' && dark ? '#e8eef1' : dark ? '#f5f5f7' : '#1c1c1e', record: '#e53935', background: values.background,
  };
}

// macOS updates its CSS AccentColor; sample it while a window is open because WebKit has no accent event.
export function watchSystemAppearance(onChange: (dark: boolean, accent: string) => void): () => void {
  const media = window.matchMedia?.('(prefers-color-scheme: dark)');
  const swatch = document.createElement('span');
  swatch.style.color = 'AccentColor';
  swatch.hidden = true;
  document.body.append(swatch);
  let last = '';
  const check = () => {
    const accent = getComputedStyle(swatch).color;
    const dark = media?.matches ?? false;
    const key = `${dark}:${accent}`;
    if (key !== last) { last = key; onChange(dark, accent); }
  };
  media?.addEventListener?.('change', check);
  const timer = window.setInterval(check, 1000);
  check();
  return () => { media?.removeEventListener?.('change', check); clearInterval(timer); swatch.remove(); };
}
