// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { mount, unmount } from 'svelte';

const downloadModel = vi.fn();
const getRecordingSettings = vi.fn();
const listAudioApps = vi.fn();
const listPrompts = vi.fn();
const modelsStatus = vi.fn();
const onModelsProgress = vi.fn();
const setRecordingSettings = vi.fn();
vi.mock('../api', () => ({
  deniedPrivacy: () => null, downloadModel, getRecordingSettings, listAudioApps, listPrompts,
  modelsStatus, onModelsProgress, openPrivacySettings: vi.fn(), setRecordingSettings,
}));
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
  listPrompts.mockResolvedValue([]);
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
  listPrompts.mockResolvedValue([]);
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
