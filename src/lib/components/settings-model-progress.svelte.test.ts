// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { mount, unmount } from 'svelte';

const downloadModel = vi.fn();
const getRecordingSettings = vi.fn();
const getAppSettings = vi.fn();
const listAudioApps = vi.fn();
const modelsStatus = vi.fn();
const onModelsProgress = vi.fn();
const setRecordingSettings = vi.fn();
const appVersion = vi.fn().mockResolvedValue({ version: '0.1.0' });
const checkForUpdates = vi.fn();
const getUpdateState = vi.fn().mockResolvedValue({ state: 'idle', installable: false });
const installUpdate = vi.fn();
const onUpdateState = vi.fn().mockResolvedValue(() => {});
const requestNotificationPermission = vi.fn();
const setAppSettings = vi.fn();
vi.mock('../api', () => ({
  appVersion, checkForUpdates, getAppSettings, getUpdateState, installUpdate, onUpdateState,
  deniedPrivacy: () => null, downloadModel, getRecordingSettings, listAudioApps,
  modelsStatus, onModelsProgress, openPrivacySettings: vi.fn(), requestNotificationPermission,
  setAppSettings, setRecordingSettings,
}));
import { appSettings } from '../app-settings';
// Covered by their own tests; stubbed so they don't share the progress mock.
vi.mock('./NoteStyles.svelte', () => ({ default: () => {} }));
vi.mock('./SummaryModel.svelte', () => ({ default: () => {} }));
const { default: Settings } = await import('./Settings.svelte');

const settings = { inputMode: 'mic', targetApp: null, multiSpeaker: false, promptId: 'default', keepAudio: false };
const downloading = { modelId: 'speaker-campplus', state: 'downloading', downloadedBytes: 4, totalBytes: 10, error: null };
const ready = { ...downloading, state: 'ready', downloadedBytes: 10 };

afterEach(() => {
  appSettings.set({ notifySummaryComplete: false, showBackgroundAnimation: true });
  vi.clearAllMocks();
});

it('applies a completion event that arrives while the status snapshot is in flight', async () => {
  let notify: ((progress: typeof ready) => void) | undefined;
  let resolveSnapshot!: (items: typeof downloading[]) => void;
  onModelsProgress.mockImplementation(async (callback) => {
    notify = callback;
    return () => {};
  });
  getRecordingSettings.mockResolvedValue(settings);
  modelsStatus.mockReturnValue(new Promise((resolve) => { resolveSnapshot = resolve; }));
  const target = document.createElement('div');
  const app = mount(Settings, { target });
  await vi.waitFor(() => expect(modelsStatus).toHaveBeenCalled());
  notify!(ready);
  resolveSnapshot([downloading]);
  await vi.waitFor(() => expect(target.textContent).toContain('Identify speakers'));
  expect(target.textContent).not.toContain('Download speaker model');
  unmount(app);
});

it('lets a newer status snapshot replace progress received before the snapshot starts', async () => {
  let notify: ((progress: typeof ready) => void) | undefined;
  let resolveSettings!: (value: typeof settings) => void;
  onModelsProgress.mockImplementation(async (callback) => {
    notify = callback;
    return () => {};
  });
  getRecordingSettings.mockReturnValue(new Promise((resolve) => { resolveSettings = resolve; }));
  modelsStatus.mockResolvedValue([ready]);
  const target = document.createElement('div');
  const app = mount(Settings, { target });
  await vi.waitFor(() => expect(notify).toBeTypeOf('function'));
  notify!(downloading);
  resolveSettings(settings);
  await vi.waitFor(() => expect(target.textContent).toContain('Identify speakers'));
  expect(target.textContent).not.toContain('Downloading speaker model…');
  unmount(app);
});

it('removes the progress listener if setup resolves after unmount', async () => {
  let resolveListener!: (remove: () => void) => void;
  const remove = vi.fn();
  onModelsProgress.mockReturnValue(new Promise((resolve) => { resolveListener = resolve; }));
  const target = document.createElement('div');
  const app = mount(Settings, { target });
  await vi.waitFor(() => expect(onModelsProgress).toHaveBeenCalled());
  unmount(app);
  resolveListener(remove);
  await vi.waitFor(() => expect(remove).toHaveBeenCalledOnce());
});

it('loads core settings and the model status snapshot when progress listener setup rejects', async () => {
  onModelsProgress.mockRejectedValue(new Error('listener unavailable'));
  getRecordingSettings.mockResolvedValue(settings);
  modelsStatus.mockResolvedValue([ready]);
  const target = document.createElement('div');
  const app = mount(Settings, { target });
  await vi.waitFor(() => expect(target.textContent).toContain('Identify speakers'));
  expect(getRecordingSettings).toHaveBeenCalledOnce();
  expect(modelsStatus).toHaveBeenCalledOnce();
  unmount(app);
});

it('shows updater state and disables restart while busy with the reason', async () => {
  onUpdateState.mockResolvedValue(() => {});
  getUpdateState.mockResolvedValue({ state: 'ready', version: '0.2.0', notes: '', installable: false });
  getRecordingSettings.mockResolvedValue(settings);
  modelsStatus.mockResolvedValue([]);
  const target = document.createElement('div');
  const app = mount(Settings, { target });
  await vi.waitFor(() => expect(target.textContent).toContain('Version 0.2.0 ready'));
  const restart = [...target.querySelectorAll('button')].find((button) => button.textContent === 'Restart to update')!;
  expect((restart as HTMLButtonElement).disabled).toBe(true);
  expect(restart.getAttribute('title')).toBe('Finish your recording or summary first');
  expect(target.textContent).toContain('Finish your recording or summary first');
  unmount(app);
});

it('keeps an update event that arrives while the update snapshot is pending', async () => {
  let emit!: (state: unknown) => void;
  let resolveSnapshot!: (state: unknown) => void;
  onUpdateState.mockImplementation(async (handler) => { emit = handler; return () => {}; });
  getUpdateState.mockReturnValue(new Promise((resolve) => { resolveSnapshot = resolve; }));
  getRecordingSettings.mockResolvedValue(settings);
  modelsStatus.mockResolvedValue([]);
  const target = document.createElement('div');
  const app = mount(Settings, { target });
  await vi.waitFor(() => expect(emit).toBeDefined());
  emit({ state: 'downloading', installable: false });
  resolveSnapshot({ state: 'idle', installable: false });
  const check = () => [...target.querySelectorAll('button')].find((b) => b.textContent === 'Check for updates') as HTMLButtonElement;
  await vi.waitFor(() => expect(check().disabled).toBe(true));
  await new Promise((r) => setTimeout(r));
  expect(check().disabled).toBe(true);
  unmount(app);
  getUpdateState.mockResolvedValue({ state: 'idle', installable: false });
});

it('shows an alert when the install is rejected', async () => {
  onUpdateState.mockResolvedValue(() => {});
  getUpdateState.mockResolvedValue({ state: 'ready', version: '0.2.0', notes: '', installable: true });
  installUpdate.mockRejectedValue('install failed');
  getRecordingSettings.mockResolvedValue(settings);
  modelsStatus.mockResolvedValue([]);
  const target = document.createElement('div');
  const app = mount(Settings, { target });
  await vi.waitFor(() => expect(target.textContent).toContain('Version 0.2.0 ready'));
  [...target.querySelectorAll('button')].find((b) => b.textContent === 'Restart to update')!.click();
  await vi.waitFor(() => expect(target.querySelector('[role="alert"]')?.textContent).toContain('install failed'));
  unmount(app);
});

it('keeps notification permission guidance through unrelated saves and shows it once', async () => {
  getAppSettings.mockResolvedValue({ notifySummaryComplete: false, showBackgroundAnimation: true });
  getRecordingSettings.mockResolvedValue(settings);
  modelsStatus.mockResolvedValue([]);
  requestNotificationPermission.mockResolvedValue(false);
  const target = document.createElement('div');
  const app = mount(Settings, { target });
  await vi.waitFor(() => expect(getRecordingSettings).toHaveBeenCalled());
  [...target.querySelectorAll('button')].find((button) => button.textContent === 'General')!.click();
  await vi.waitFor(() => expect(target.textContent).toContain('Show summarization complete alert'));
  const alerts = () => [...target.querySelectorAll('[role="status"]')];
  const notification = [...target.querySelectorAll('input')].find((input) => input.parentElement?.textContent?.includes('Show summarization complete alert'))!;
  notification.checked = true;
  notification.dispatchEvent(new Event('change', { bubbles: true }));
  await vi.waitFor(() => expect(requestNotificationPermission).toHaveBeenCalledOnce());
  await vi.waitFor(() => expect(alerts()).toHaveLength(1));

  const animation = [...target.querySelectorAll('input')].find((input) => input.parentElement?.textContent?.includes('Show background animation'))!;
  animation.checked = false;
  animation.dispatchEvent(new Event('change', { bubbles: true }));
  await vi.waitFor(() => expect(setAppSettings).toHaveBeenCalled());
  expect(alerts()).toHaveLength(1);
  [...target.querySelectorAll('button')].find((button) => button.textContent === 'Session')!.click();
  expect(alerts()).toHaveLength(1);
  unmount(app);
});
