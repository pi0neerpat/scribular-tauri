import { beforeEach, describe, expect, it, vi } from 'vitest';
import { get } from 'svelte/store';
import { customize, defaultAppearance } from './appearance';
import type { Appearance } from './types/Appearance';
import type { Background } from './types/Background';

const api = vi.hoisted(() => ({ getAppearance: vi.fn(), onAppearanceChanged: vi.fn(), setAppearance: vi.fn(), chooseBackgroundPicture: vi.fn(), importBackgroundPicture: vi.fn() }));
vi.mock('./api', () => api);
vi.mock('./appearance', async (importOriginal) => ({ ...await importOriginal<typeof import('./appearance')>(), watchSystemAppearance: () => () => {} }));

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

describe('appearance saves', () => {
  beforeEach(() => { vi.resetModules(); Object.values(api).forEach((fn) => fn.mockReset()); });

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
    expect(api.setAppearance).toHaveBeenNthCalledWith(1, sage);
    event(v1);
    first.resolve(sage);
    await saveFirst;
    expect(get(store.appearance)).toEqual(v1);
    expect(api.setAppearance).toHaveBeenCalledTimes(2);
    expect(api.setAppearance).toHaveBeenNthCalledWith(2, v1);
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

  const mine = (background: Background): Appearance => {
    const theme = customize(defaultAppearance, 'default', true).myTheme!;
    return { ...defaultAppearance, selected: 'myTheme', myTheme: { ...theme, values: { ...theme.values, background } } };
  };
  async function started(initial: Appearance) {
    api.onAppearanceChanged.mockResolvedValue(() => {});
    api.getAppearance.mockResolvedValue(initial);
    const store = await import('./appearance-store');
    return { store, stop: await store.startAppearance() };
  }

  it('applies a chosen picture and keeps the theme when an import is rejected', async () => {
    const { store, stop } = await started(mine('none'));
    api.chooseBackgroundPicture.mockResolvedValue(mine({ owned: 'abc' }));
    await store.chooseBackground();
    expect(get(store.appearance)).toEqual(mine({ owned: 'abc' }));
    expect(get(store.appearanceError)).toBeNull();
    api.importBackgroundPicture.mockRejectedValue("Scribular can't open this picture.");
    await store.dropBackground('/tmp/a.txt');
    expect(api.importBackgroundPicture).toHaveBeenCalledWith('/tmp/a.txt');
    expect(get(store.appearance)).toEqual(mine({ owned: 'abc' }));
    expect(get(store.appearanceError)).toBe("Scribular can't open this picture.");
    stop();
  });

  it('applies a dropped picture through the import command', async () => {
    const { store, stop } = await started(mine('none'));
    api.importBackgroundPicture.mockResolvedValue(mine({ owned: 'def' }));
    await store.dropBackground('/Users/me/sky.heic');
    expect(api.importBackgroundPicture).toHaveBeenCalledWith('/Users/me/sky.heic');
    expect(get(store.appearance)).toEqual(mine({ owned: 'def' }));
    stop();
  });

  it('rebases an edit queued behind a picture import on the imported picture', async () => {
    const { store, stop } = await started(mine('none'));
    const importing = deferred<Appearance>();
    api.importBackgroundPicture.mockReturnValue(importing.promise);
    api.setAppearance.mockImplementation(async (next: Appearance) => next);
    const drop = store.dropBackground('/Users/me/sky.png');
    const edit = store.saveAppearance((latest) => ({ ...latest, myTheme: { ...latest.myTheme!, values: { ...latest.myTheme!.values, blur: 40 } } }));
    importing.resolve(mine({ owned: 'abc' }));
    await drop;
    await edit;
    expect(api.setAppearance).toHaveBeenCalledOnce();
    expect(get(store.appearance).myTheme!.values).toMatchObject({ background: { owned: 'abc' }, blur: 40 });
    stop();
  });

  it('does not show a stale picture failure after a newer event', async () => {
    let event!: (next: Appearance) => void;
    api.onAppearanceChanged.mockImplementation(async (handler) => { event = handler; return () => {}; });
    api.getAppearance.mockResolvedValue(mine('none'));
    const pending = deferred<Appearance>();
    api.chooseBackgroundPicture.mockReturnValue(pending.promise);
    const store = await import('./appearance-store');
    const stop = await store.startAppearance();
    const choosing = store.chooseBackground();
    await Promise.resolve();
    event(mine('stars'));
    pending.reject(new Error('old failure'));
    await choosing;
    expect(get(store.appearance)).toEqual(mine('stars'));
    expect(get(store.appearanceError)).toBeNull();
    stop();
  });
});
