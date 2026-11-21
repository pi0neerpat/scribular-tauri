import { describe, expect, it } from 'vitest';
import { customize, defaultAppearance, presetValues, resetMyTheme, resolveAppearance, selectPreset } from './appearance';
import type { Appearance } from './types/Appearance';
import type { Tint } from './types/Tint';

describe('appearance state', () => {
  it('resolves built-ins under system and explicit mode changes', () => {
    for (const selected of ['default', 'sage', 'scribularV1'] as const) {
      const state: Appearance = { ...defaultAppearance, selected };
      const light = resolveAppearance(state, false, '#123456');
      const dark = resolveAppearance(state, true, '#123456');
      expect(dark.dark).toBe(true);
      expect(light.dark).toBe(selected === 'scribularV1');
      expect(light.modeEditable).toBe(selected !== 'scribularV1');
      expect(resolveAppearance({ ...state, mode: 'light' }, true, '#123456').dark).toBe(selected === 'scribularV1');
      expect(resolveAppearance({ ...state, mode: 'dark' }, false, '#123456').dark).toBe(true);
    }
  });

  it('preserves My Theme, canceling replacement, and resets to its origin', () => {
    const copied = customize(defaultAppearance, 'sage');
    const edited: Appearance = { ...copied, myTheme: { origin: 'sage', values: { ...presetValues('sage'), tint: 'rose' } } };
    expect(selectPreset(selectPreset(edited, 'default'), 'myTheme')).toEqual(edited);
    expect(customize(edited, 'scribularV1')).toEqual(edited);
    expect(customize(edited, 'scribularV1', true).myTheme).toEqual({ origin: 'scribularV1', values: presetValues('scribularV1') });
    expect(resetMyTheme(edited)).toEqual(copied);
  });

  it('updates accents and every tint without changing text or record colors', () => {
    const base = customize(defaultAppearance, 'default');
    const values = base.myTheme!.values;
    const withValues = (patch: Partial<typeof values>): Appearance => ({ ...base, myTheme: { origin: 'default', values: { ...values, ...patch } } });
    expect(resolveAppearance(base, false, '#123456').accent).toBe('#123456');
    expect(resolveAppearance(base, false, '#654321').accent).toBe('#654321');
    expect(resolveAppearance(withValues({ accent: 'purple' }), false, '#123456').accent).toBe('#af52de');
    expect(resolveAppearance(withValues({ accent: { custom: '#abcdef' } }), false, '#123456').accent).toBe('#abcdef');
    const reference = resolveAppearance(base, false, '#123456');
    const backgrounds = new Set<string>();
    for (const tint of ['none', 'warm', 'cool', 'sage', 'rose'] as Tint[]) {
      const resolved = resolveAppearance(withValues({ tint }), false, '#123456');
      backgrounds.add(resolved.tint);
      expect(resolved.text).toBe(reference.text);
      expect(resolved.record).toBe(reference.record);
    }
    expect(backgrounds.size).toBe(5);
  });
});
