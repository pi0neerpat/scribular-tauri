// @vitest-environment jsdom
import { afterEach, expect, it, vi } from 'vitest';
import { mount, unmount } from 'svelte';
import type { ShortcutSettings } from '../types/ShortcutSettings';

const saved: ShortcutSettings = { globalShortcutsEnabled: true, toggleRecording: 'control+alt+super+KeyR' };
let active: string | null = saved.toggleRecording;
const shortcutRegistrationState = vi.fn(async () => ({ settings: saved, activeChord: active, error: null }));
const getShortcutBindings = vi.fn(async () => [['shell-record', 'CmdOrCtrl+R']]);
const setShortcutCapture = vi.fn(async (capturing: boolean) => {
  active = capturing ? null : saved.toggleRecording;
  return { settings: saved, activeChord: active, error: null };
});
const setShortcutSettings = vi.fn(async (settings: typeof saved) => {
  saved.toggleRecording = settings.toggleRecording;
  active = settings.toggleRecording;
  return { settings: { ...settings }, activeChord: active, error: null };
});
vi.mock('../api', () => ({ shortcutRegistrationState, getShortcutBindings, setShortcutCapture, setShortcutSettings, onShortcutRegistration: vi.fn(async () => () => {}) }));
const { default: ShortcutsSettings } = await import('./ShortcutsSettings.svelte');

let app: ReturnType<typeof mount>;
let target: HTMLDivElement;
afterEach(() => { if (app) unmount(app); target?.remove(); vi.clearAllMocks(); saved.toggleRecording = 'control+alt+super+KeyR'; active = saved.toggleRecording; });

it('restores registration after cancel and saves a physical key or clear', async () => {
  target = document.body.appendChild(document.createElement('div'));
  app = mount(ShortcutsSettings, { target });
  const field = await vi.waitFor(() => {
    const element = target.querySelector<HTMLButtonElement>('#toggle-recording-shortcut');
    expect(element).not.toBeNull();
    return element!;
  });
  field.focus();
  await vi.waitFor(() => expect(setShortcutCapture).toHaveBeenCalledWith(true));
  field.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
  await vi.waitFor(() => expect(setShortcutCapture).toHaveBeenCalledWith(false));
  field.blur(); field.focus();
  await vi.waitFor(() => expect(setShortcutCapture).toHaveBeenCalledTimes(3));
  field.dispatchEvent(new KeyboardEvent('keydown', { key: 'k', code: 'KeyK', ctrlKey: true, altKey: true, bubbles: true }));
  await vi.waitFor(() => expect(setShortcutSettings).toHaveBeenCalledWith({ globalShortcutsEnabled: true, toggleRecording: 'Ctrl+Alt+KeyK' }));
  await vi.waitFor(() => expect(field.disabled).toBe(false));
  field.blur(); field.focus();
  await vi.waitFor(() => expect(setShortcutCapture).toHaveBeenCalledTimes(4));
  field.dispatchEvent(new KeyboardEvent('keydown', { key: 'Delete', code: 'Delete', bubbles: true }));
  await vi.waitFor(() => expect(setShortcutSettings).toHaveBeenCalledWith({ globalShortcutsEnabled: true, toggleRecording: null }));
});

it('restores the saved chord when capture loses focus', async () => {
  target = document.body.appendChild(document.createElement('div'));
  app = mount(ShortcutsSettings, { target });
  const field = await vi.waitFor(() => {
    const element = target.querySelector<HTMLButtonElement>('#toggle-recording-shortcut');
    expect(element).not.toBeNull();
    return element!;
  });
  field.focus();
  await vi.waitFor(() => expect(setShortcutCapture).toHaveBeenCalledWith(true));
  field.blur();
  await vi.waitFor(() => expect(setShortcutCapture).toHaveBeenCalledWith(false));
  expect(active).toBe(saved.toggleRecording);
});

it('cancels Command+Period without saving a chord', async () => {
  target = document.body.appendChild(document.createElement('div'));
  app = mount(ShortcutsSettings, { target });
  const field = await vi.waitFor(() => { const element = target.querySelector<HTMLButtonElement>('#toggle-recording-shortcut'); expect(element).not.toBeNull(); return element!; });
  field.focus();
  await vi.waitFor(() => expect(setShortcutCapture).toHaveBeenCalledWith(true));
  field.dispatchEvent(new KeyboardEvent('keydown', { key: '.', code: 'Period', metaKey: true, bubbles: true }));
  await vi.waitFor(() => expect(setShortcutCapture).toHaveBeenCalledWith(false));
  expect(setShortcutSettings).not.toHaveBeenCalled();
  expect(target.querySelector('[role="alert"]')).toBeNull();
});

it('restores registration if focus leaves before capture starts', async () => {
  let finish!: (value: { settings: ShortcutSettings; activeChord: string | null; error: null }) => void;
  setShortcutCapture.mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }));
  target = document.body.appendChild(document.createElement('div'));
  app = mount(ShortcutsSettings, { target });
  const field = await vi.waitFor(() => { const element = target.querySelector<HTMLButtonElement>('#toggle-recording-shortcut'); expect(element).not.toBeNull(); return element!; });
  field.focus();
  await vi.waitFor(() => expect(setShortcutCapture).toHaveBeenCalledWith(true));
  field.blur();
  finish({ settings: saved, activeChord: null, error: null });
  await vi.waitFor(() => expect(setShortcutCapture).toHaveBeenCalledWith(false));
});

it('restores registration if the pane closes before capture starts', async () => {
  let finish!: (value: { settings: ShortcutSettings; activeChord: string | null; error: null }) => void;
  setShortcutCapture.mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }));
  target = document.body.appendChild(document.createElement('div'));
  app = mount(ShortcutsSettings, { target });
  const field = await vi.waitFor(() => { const element = target.querySelector<HTMLButtonElement>('#toggle-recording-shortcut'); expect(element).not.toBeNull(); return element!; });
  field.focus();
  await vi.waitFor(() => expect(setShortcutCapture).toHaveBeenCalledWith(true));
  unmount(app);
  finish({ settings: saved, activeChord: null, error: null });
  await vi.waitFor(() => expect(setShortcutCapture).toHaveBeenCalledWith(false));
});
