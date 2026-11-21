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
    expect(resolveAppearance({ ...defaultAppearance, selected: 'scribularV1' }, false, '#123456')).toMatchObject({ accent: '#057dcd', tint: '#000000', text: '#e8eef1' });
    expect(resolveAppearance({ ...defaultAppearance, selected: 'sage' }, false, '#123456')).toMatchObject({ accent: '#467c63', tint: '#fcfbf8' });
    expect(resolveAppearance({ ...defaultAppearance, selected: 'sage' }, true, '#123456')).toMatchObject({ accent: '#59a682', tint: '#110f0d' });
  });

  it('preserves My Theme, canceling replacement, and resets to its origin', () => {
    const copied = customize(defaultAppearance, 'sage');
    const edited: Appearance = { ...copied, myTheme: { ...copied.myTheme!, values: { ...copied.myTheme!.values, tint: 'rose' } } };
    const selectedDefault = selectPreset(edited, 'default');
    expect(selectedDefault.selected).toBe('default');
    expect(selectedDefault.myTheme).toEqual(edited.myTheme);
    expect(selectPreset(selectedDefault, 'myTheme')).toEqual(edited);
    expect(customize(edited, 'scribularV1')).toEqual(edited);
    expect(customize(edited, 'scribularV1', true).myTheme).toEqual({ origin: 'scribularV1', copiedMode: 'dark', values: presetValues('scribularV1') });
    expect(resetMyTheme(edited)).toEqual(copied);
    for (const mode of ['light', 'dark'] as const) {
      const override = customize({ ...defaultAppearance, mode }, 'sage');
      expect(override.myTheme?.values.mode).toBe(mode);
      const changed: Appearance = { ...override, myTheme: { ...override.myTheme!, values: { ...override.myTheme!.values, mode: 'automatic' } } };
      expect(resetMyTheme(changed).myTheme?.values.mode).toBe(mode);
    }
  });

  it('updates accents and every tint without changing text or record colors', () => {
    const base = customize(defaultAppearance, 'default');
    const values = base.myTheme!.values;
    const withValues = (patch: Partial<typeof values>): Appearance => ({ ...base, myTheme: { ...base.myTheme!, values: { ...values, ...patch } } });
    expect(resolveAppearance(base, false, '#123456').accent).toBe('#123456');
    expect(resolveAppearance(base, false, '#654321').accent).toBe('#654321');
    expect(resolveAppearance(withValues({ accent: 'purple' }), false, '#123456').accent).toBe('#af52de');
    expect(resolveAppearance(withValues({ accent: { custom: '#abcdef' } }), false, '#123456').accent).toBe('#abcdef');
    const reference = resolveAppearance(base, false, '#123456');
    const expected = {
      none: ['#ffffff', '#1c1c1e'], warm: ['#fff2e6', '#33271f'], cool: ['#edf4ff', '#1d2938'],
      sage: ['#fcfbf8', '#110f0d'], rose: ['#fff0f3', '#38252d'],
    } as const;
    for (const tint of ['none', 'warm', 'cool', 'sage', 'rose'] as Tint[]) {
      for (const [dark, token] of [[false, expected[tint][0]], [true, expected[tint][1]]] as const) {
        const resolved = resolveAppearance(withValues({ tint }), dark, '#123456');
        expect(resolved.tint).toBe(token);
        expect(resolved.text).toBe(resolveAppearance(base, dark, '#123456').text);
        expect(resolved.record).toBe(reference.record);
      }
    }
  });

  it('keeps copied preset colors while allowing color and mode edits', () => {
    for (const preset of ['sage', 'scribularV1'] as const) {
      const source = { ...defaultAppearance, selected: preset };
      const copied = customize(source, preset);
      for (const dark of [false, true]) {
        const builtIn = resolveAppearance(source, dark, '#123456');
        expect(resolveAppearance(copied, dark, '#123456')).toMatchObject({
          dark: builtIn.dark, accent: builtIn.accent, tint: builtIn.tint, text: builtIn.text,
          record: builtIn.record, background: builtIn.background,
        });
      }
      const edited: Appearance = { ...copied, myTheme: { ...copied.myTheme!, values: { ...copied.myTheme!.values, accent: 'purple', tint: 'rose', mode: 'light' } } };
      expect(resolveAppearance(edited, true, '#123456')).toMatchObject({ dark: false, modeEditable: true, accent: '#af52de', tint: '#fff0f3' });
    }
  });
});
