import { beforeEach, describe, expect, it, vi } from 'vitest';
import { get } from 'svelte/store';
import { defaultAppearance } from './appearance';
import type { Appearance } from './types/Appearance';

const api = vi.hoisted(() => ({ getAppearance: vi.fn(), onAppearanceChanged: vi.fn(), setAppearance: vi.fn() }));
vi.mock('./api', () => api);
vi.mock('./appearance', async (importOriginal) => ({ ...await importOriginal<typeof import('./appearance')>(), watchSystemAppearance: () => () => {} }));

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

describe('appearance saves', () => {
  beforeEach(() => { vi.resetModules(); api.getAppearance.mockReset(); api.onAppearanceChanged.mockReset(); api.setAppearance.mockReset(); });

  it('serializes overlapping saves and keeps a newer event after a stale response', async () => {
    let event!: (next: Appearance) => void;
    api.onAppearanceChanged.mockImplementation(async (handler) => { event = handler; return () => {}; });
    api.getAppearance.mockResolvedValue(defaultAppearance);
    const first = deferred<Appearance>();
    const second = deferred<Appearance>();
    api.setAppearance.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
    const store = await import('./appearance-store');
    const stop = await store.startAppearance();
    const sage: Appearance = { ...defaultAppearance, selected: 'sage' };
    const v1: Appearance = { ...defaultAppearance, selected: 'scribularV1' };
    const saveFirst = store.saveAppearance(sage);
    const saveSecond = store.saveAppearance(v1);
    await Promise.resolve();
    expect(api.setAppearance).toHaveBeenCalledTimes(1);
    event(v1);
    first.resolve(sage);
    await saveFirst;
    expect(get(store.appearance)).toEqual(v1);
    expect(api.setAppearance).toHaveBeenCalledTimes(2);
    second.resolve(v1);
    await saveSecond;
    expect(get(store.appearance)).toEqual(v1);
    stop();
  });

  it('does not show an obsolete failure after a newer event', async () => {
    let event!: (next: Appearance) => void;
    api.onAppearanceChanged.mockImplementation(async (handler) => { event = handler; return () => {}; });
    api.getAppearance.mockResolvedValue(defaultAppearance);
    const pending = deferred<Appearance>();
    api.setAppearance.mockReturnValue(pending.promise);
    const store = await import('./appearance-store');
    const stop = await store.startAppearance();
    const sage: Appearance = { ...defaultAppearance, selected: 'sage' };
    const saving = store.saveAppearance(sage);
    await Promise.resolve();
    event(sage);
    pending.reject(new Error('old failure'));
    await saving;
    expect(get(store.appearance)).toEqual(sage);
    expect(get(store.appearanceError)).toBeNull();
    stop();
  });
});
