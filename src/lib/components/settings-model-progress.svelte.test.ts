// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { mount, unmount } from 'svelte';

const downloadModel = vi.fn();
const getRecordingSettings = vi.fn();
const listAudioApps = vi.fn();
const modelsStatus = vi.fn();
const onModelsProgress = vi.fn();
const setRecordingSettings = vi.fn();
const appVersion = vi.fn().mockResolvedValue({ version: '0.1.0' });
const checkForUpdates = vi.fn();
const getUpdateState = vi.fn().mockResolvedValue({ state: 'idle', installable: false });
const installUpdate = vi.fn();
const onUpdateState = vi.fn().mockResolvedValue(() => {});
vi.mock('../api', () => ({
  appVersion, checkForUpdates, getUpdateState, installUpdate, onUpdateState,
  deniedPrivacy: () => null, downloadModel, getRecordingSettings, listAudioApps,
  modelsStatus, onModelsProgress, openPrivacySettings: vi.fn(), setRecordingSettings,
}));
// Covered by their own tests; stubbed so they don't share the progress mock.
vi.mock('./NoteStyles.svelte', () => ({ default: () => {} }));
vi.mock('./SummaryModel.svelte', () => ({ default: () => {} }));
const { default: Settings } = await import('./Settings.svelte');

const settings = { inputMode: 'mic', targetApp: null, multiSpeaker: false, promptId: 'default', keepAudio: false };
const downloading = { modelId: 'speaker-campplus', state: 'downloading', downloadedBytes: 4, totalBytes: 10, error: null };
const ready = { ...downloading, state: 'ready', downloadedBytes: 10 };

afterEach(() => vi.clearAllMocks());

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
