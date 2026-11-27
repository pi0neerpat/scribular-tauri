// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';

const downloadModel = vi.fn();
const getRecordingSettings = vi.fn();
const getAppSettings = vi.fn();
const listAudioApps = vi.fn();
const modelsStatus = vi.fn();
const onModelsProgress = vi.fn();
const setRecordingSettings = vi.fn();
const setRecordingPreferences = vi.fn();
const appVersion = vi.fn().mockResolvedValue({ version: '0.1.0' });
const checkForUpdates = vi.fn();
const getUpdateState = vi.fn().mockResolvedValue({ state: 'idle', installable: false });
const installUpdate = vi.fn();
const onUpdateState = vi.fn().mockResolvedValue(() => {});
const requestNotificationPermission = vi.fn();
const setAppSettings = vi.fn();
const getTelemetrySetting = vi.fn().mockResolvedValue('off');
const getSummaryAvailability = vi.fn().mockResolvedValue('ready');
const onSummaryAvailability = vi.fn().mockResolvedValue(() => {});
const subscribeSummaryAvailability = vi.fn(async (callback: (state: string) => void, _onError: (error: unknown) => void) => {
  callback(await getSummaryAvailability());
  return () => {};
});
const retrySummaryModel = vi.fn();
vi.mock('../api', () => ({
  appVersion, checkForUpdates, getAppSettings, getUpdateState, installUpdate, onUpdateState,
  deniedPrivacy: () => null, downloadModel, getRecordingSettings, listAudioApps,
  modelsStatus, onModelsProgress, openPrivacySettings: vi.fn(), requestNotificationPermission,
  setAppSettings, setRecordingSettings, setRecordingPreferences, getTelemetrySetting,
  getSummaryAvailability, onSummaryAvailability, subscribeSummaryAvailability, retrySummaryModel,
  onSettingsPane: vi.fn().mockResolvedValue(() => {}),
  onFileDrop: vi.fn().mockResolvedValue(() => {}),
}));
import { appSettings } from '../app-settings';
// Covered by their own tests; stubbed so they don't share the progress mock.
vi.mock('./NoteStyles.svelte', () => ({ default: () => {} }));
const { default: Settings } = await import('./Settings.svelte');

const settings = { inputMode: 'mic', targetApp: null, multiSpeaker: false, promptId: 'default', keepAudio: false };
const downloading = { modelId: 'speaker-campplus', state: 'downloading', downloadedBytes: 4, totalBytes: 10, error: null };
const ready = { ...downloading, state: 'ready', downloadedBytes: 10 };

beforeEach(() => {
  window.history.replaceState({}, '', '?pane=Recording');
  getAppSettings.mockResolvedValue({ notifySummaryComplete: false, showBackgroundAnimation: true, checkForUpdatesAutomatically: true });
});

afterEach(() => {
  window.history.replaceState({}, '', '/');
  appSettings.set({ notifySummaryComplete: false, showBackgroundAnimation: true, checkForUpdatesAutomatically: true });
  vi.clearAllMocks();
});

it('opens on General and keeps recording and appearance controls in their panes', async () => {
  window.history.replaceState({}, '', '/');
  getSummaryAvailability.mockResolvedValueOnce('downloading');
  getRecordingSettings.mockResolvedValue(settings);
  modelsStatus.mockResolvedValue([]);
  const target = document.createElement('div');
  const app = mount(Settings, { target });
  await vi.waitFor(() => expect(target.textContent).toContain('Show summarization complete alert'));
  expect(target.textContent).not.toContain('Identify speakers');
  expect(target.textContent).not.toContain('Background picture');
  expect(target.textContent).not.toContain('Restore sample session');
  expect(target.textContent).not.toContain("Summaries aren't available on this Mac.");
  [...target.querySelectorAll('button')].find((button) => button.textContent === 'Appearance')!.click();
  flushSync();
  expect(target.textContent).toContain('Background picture');
  expect(target.textContent).not.toContain('Show background animation');
  [...target.querySelectorAll('button')].find((button) => button.textContent === 'Recording')!.click();
  flushSync();
  expect(target.textContent).toContain('Multi-speaker');
  unmount(app);
});

it('keeps source controls off Settings and saves only recording preferences', async () => {
  onModelsProgress.mockResolvedValue(() => {});
  getRecordingSettings.mockResolvedValue(settings);
  modelsStatus.mockResolvedValue([]);
  listAudioApps.mockResolvedValue([{ name: 'Zoom', sampleRate: 48000, channels: 2, icon: null }]);
  const target = document.createElement('div');
  const app = mount(Settings, { target });
  await vi.waitFor(() => expect(target.textContent).toContain('Multi-speaker'));
  expect(target.querySelector('input[value="micAndSystem"]')).toBeNull();
  expect(target.querySelector('input[value="mic"]')).toBeNull();
  expect(target.querySelector('select')).toBeNull();
  expect(listAudioApps).not.toHaveBeenCalled();
  const keepAudio = target.querySelector<HTMLInputElement>('input[type="radio"][value="true"]')!;
  keepAudio.checked = true;
  keepAudio.dispatchEvent(new Event('change', { bubbles: true }));
  await vi.waitFor(() => {
    expect(setRecordingPreferences).toHaveBeenCalledTimes(1);
    expect(setRecordingPreferences).toHaveBeenCalledWith(true, false);
  });
  expect(setRecordingSettings).not.toHaveBeenCalled();
  unmount(app);
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

it('shows the plain summary limitation and retry on a load failure', async () => {
  getSummaryAvailability.mockResolvedValueOnce('loadFailed');
  getRecordingSettings.mockResolvedValue(settings);
  modelsStatus.mockResolvedValue([ready]);
  const target = document.createElement('div');
  const app = mount(Settings, { target });
  await vi.waitFor(() => expect(target.textContent).toContain("Summaries couldn't load."));
  expect(target.textContent).not.toContain("aren't available on this Mac");
  const retries = [...target.querySelectorAll('button')].filter((button) => button.textContent === 'Try Again');
  expect(retries).toHaveLength(1);
  retries[0].click();
  expect(retrySummaryModel).toHaveBeenCalledOnce();
  unmount(app);
});

it('shows and retries a summary availability read error', async () => {
  subscribeSummaryAvailability.mockImplementationOnce(async (_callback, onError) => {
    onError(new Error('IPC failed'));
    return () => {};
  });
  getRecordingSettings.mockResolvedValue(settings);
  modelsStatus.mockResolvedValue([ready]);
  const target = document.createElement('div');
  const app = mount(Settings, { target });
  await vi.waitFor(() => expect(target.textContent).toContain("Couldn't check summary availability."));
  expect(target.textContent).not.toContain('Waiting for the summary model download');
  subscribeSummaryAvailability.mockImplementationOnce(async (callback) => {
    callback('ready');
    return () => {};
  });
  [...target.querySelectorAll('button')].find((button) => button.textContent === 'Check again')!.click();
  await vi.waitFor(() => expect(target.textContent).not.toContain("Couldn't check summary availability."));
  unmount(app);
});

it('shows updater state and disables restart while busy with the reason', async () => {
  window.history.replaceState({}, '', '?pane=General');
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
  window.history.replaceState({}, '', '?pane=General');
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

it('restores the persisted automatic-update switch after a failed save', async () => {
  window.history.replaceState({}, '', '?pane=General');
  getAppSettings.mockResolvedValue({ notifySummaryComplete: false, showBackgroundAnimation: true, checkForUpdatesAutomatically: true });
  setAppSettings.mockRejectedValueOnce('save failed');
  getRecordingSettings.mockResolvedValue(settings);
  modelsStatus.mockResolvedValue([]);
  const target = document.createElement('div');
  const app = mount(Settings, { target });
  const automatic = () => [...target.querySelectorAll('input')].find((input) => input.parentElement?.textContent?.includes('Check for updates automatically')) as HTMLInputElement;
  await vi.waitFor(() => expect(automatic()?.disabled).toBe(false));
  expect(automatic().checked).toBe(true);
  automatic().checked = false;
  automatic().dispatchEvent(new Event('change', { bubbles: true }));
  await vi.waitFor(() => expect(target.querySelector('[role="alert"]')?.textContent).toContain('save failed'));
  await vi.waitFor(() => expect(automatic().checked).toBe(true));
  unmount(app);
});

it('shows an alert when the install is rejected', async () => {
  window.history.replaceState({}, '', '?pane=General');
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
  getAppSettings.mockResolvedValue({ notifySummaryComplete: false, showBackgroundAnimation: true, checkForUpdatesAutomatically: true });
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
  const updates = [...target.querySelectorAll('input')].find((input) => input.parentElement?.textContent?.includes('Check for updates automatically'))!;
  await vi.waitFor(() => expect(updates.disabled).toBe(false));
  setAppSettings.mockClear();
  updates.checked = false;
  updates.dispatchEvent(new Event('change', { bubbles: true }));
  await vi.waitFor(() => expect(setAppSettings).toHaveBeenCalledWith(expect.objectContaining({ checkForUpdatesAutomatically: false })));

  [...target.querySelectorAll('button')].find((button) => button.textContent === 'Appearance')!.click();
  flushSync();
  expect(alerts()).toHaveLength(1);
  [...target.querySelectorAll('button')].find((button) => button.textContent === 'Recording')!.click();
  await vi.waitFor(() => expect(target.textContent).toContain('Notifications are disabled. Allow them for Scribular in System Settings.'));
  unmount(app);
});
