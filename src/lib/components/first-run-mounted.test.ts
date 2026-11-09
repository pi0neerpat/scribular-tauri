// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { mount, unmount } from 'svelte';

const { getSummaryAvailability, onSummaryAvailability } = vi.hoisted(() => ({
  getSummaryAvailability: vi.fn(),
  onSummaryAvailability: vi.fn().mockResolvedValue(() => {}),
}));
vi.mock('../api', async (importOriginal) => ({
  ...await importOriginal<typeof import('../api')>(),
  downloadModels: vi.fn().mockResolvedValue(undefined),
  getSummaryAvailability,
  getTelemetrySetting: vi.fn().mockResolvedValue('off'),
  modelsStatus: vi.fn().mockResolvedValue([{ modelId: 'whisper-base', state: 'ready', downloadedBytes: 10, totalBytes: 10, error: null }]),
  onModelsProgress: vi.fn().mockResolvedValue(() => {}),
  onSummaryAvailability,
  openPrivacySettings: vi.fn(),
  subscribeSummaryAvailability: async (callback: (state: string) => void) => {
    const unlisten = await onSummaryAvailability(callback);
    callback(await getSummaryAvailability());
    return unlisten;
  },
}));
const { default: FirstRun } = await import('./FirstRun.svelte');

afterEach(() => vi.clearAllMocks());

describe('first-run summary availability', () => {
  for (const state of ['unsupported', 'loadFailed'] as const) {
    it(`shows the unavailable message for ${state}`, async () => {
      getSummaryAvailability.mockResolvedValueOnce(state);
      const target = document.createElement('div');
      const app = mount(FirstRun, { target, props: { speechReady: true } });
      await vi.waitFor(() => expect(target.textContent).toContain("Summaries aren't available on this Mac."));
      unmount(app);
    });
  }

  it('shows the download wait state', async () => {
    getSummaryAvailability.mockResolvedValueOnce('downloading');
    const target = document.createElement('div');
    const app = mount(FirstRun, { target, props: { speechReady: true } });
    await vi.waitFor(() => expect(target.textContent).toContain('Waiting for the summary model download'));
    unmount(app);
  });
});
