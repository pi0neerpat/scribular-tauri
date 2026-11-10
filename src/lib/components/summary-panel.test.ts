// @vitest-environment jsdom
import { expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';
import type { Session } from '../types/Session';

const api = vi.hoisted(() => ({
  cancelJob: vi.fn(), copyText: vi.fn(), getPromptList: vi.fn(), listSummaryModels: vi.fn(),
  getSummaryAvailability: vi.fn().mockResolvedValue('ready'), onSummaryAvailability: vi.fn(async () => () => {}),
  subscribeSummaryAvailability: vi.fn(async (callback: (state: string) => void, _onError: (error: unknown) => void) => { callback(await api.getSummaryAvailability()); return () => {}; }),
  onSummaryDelta: vi.fn(), regenerateSummary: vi.fn(), undoSummary: vi.fn(), delta: undefined as ((d: unknown) => void) | undefined,
}));
vi.mock('../api', () => ({
  ...api,
  onSummaryDelta: (handler: (d: unknown) => void) => { api.delta = handler; return Promise.resolve(() => {}); },
}));
const { default: SummaryPanel } = await import('./SummaryPanel.svelte');

const session: Session = {
  id: 'session-1', title: 'Session', createdAt: 1, inputMode: 'mic', status: 'done', error: null,
  promptId: 'prompt-1', modelId: null, transcript: 'transcript text', plainTranscript: null, rawTranscript: null,
  speakerMap: null, diarizationState: 'none', summary: 'saved summary', durationMs: 1000, timings: null,
  hasAudio: false, audioExpiresAt: null, previousSummary: null, previousPromptId: null, previousModelId: null, isSample: false,
};
const prompts = { prompts: [{ id: 'prompt-1', name: 'Style', description: '', text: '', isDefault: false, starred: true }], activeId: 'prompt-1', createdId: null };

it('copies the saved summary, reports success, and keeps streamed regeneration text in preview', async () => {
  const writeText = vi.fn().mockResolvedValue(undefined);
  api.listSummaryModels.mockResolvedValue([]);
  Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText } });
  const target = document.createElement('div');
  const app = mount(SummaryPanel, { target, props: { session, management: true, prompts, models: [] } });
  target.querySelector('button')!.click();
  await vi.waitFor(() => expect(writeText).toHaveBeenCalledWith('saved summary'));
  await vi.waitFor(() => expect(target.textContent).toContain('Copied'));
  target.querySelectorAll('button')[1].click();
  await vi.waitFor(() => expect(api.regenerateSummary).toHaveBeenCalledWith('session-1', 'prompt-1'));
  api.delta?.({ sessionId: 'session-1', text: 'new preview' });
  flushSync();
  expect(target.textContent).toContain('saved summary');
  expect(target.textContent).toContain('new preview');
  unmount(app);
});

it('disables Copy when both summary and transcript are empty', () => {
  api.listSummaryModels.mockResolvedValue([]);
  const target = document.createElement('div');
  const app = mount(SummaryPanel, { target, props: {
    session: { ...session, summary: '', transcript: '' }, management: true, prompts, models: [],
  } });
  expect([...target.querySelectorAll('button')].find((b) => b.textContent === 'Copy')?.disabled).toBe(true);
  unmount(app);
});

it('copies the transcript when the saved summary is empty', async () => {
  api.listSummaryModels.mockResolvedValue([]);
  const writeText = vi.fn().mockResolvedValue(undefined);
  Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText } });
  const target = document.createElement('div');
  const app = mount(SummaryPanel, { target, props: {
    session: { ...session, summary: '' }, management: true, prompts, models: [],
  } });
  target.querySelector('button')!.click();
  await vi.waitFor(() => expect(writeText).toHaveBeenCalledWith('transcript text'));
  unmount(app);
});

it('keeps an existing summary visible and disables regeneration when summaries are unavailable', async () => {
  api.getSummaryAvailability.mockResolvedValueOnce('loadFailed');
  api.listSummaryModels.mockResolvedValue([]);
  const target = document.createElement('div');
  const app = mount(SummaryPanel, { target, props: { session, management: true, prompts, models: [] } });
  await vi.waitFor(() => expect(target.textContent).toContain("Summaries aren't available on this Mac."));
  expect(target.textContent).toContain('saved summary');
  expect([...target.querySelectorAll('button')].find((b) => b.textContent === 'Regenerate')?.disabled).toBe(true);
  unmount(app);
});

it('shows and retries an availability error in the normal record view', async () => {
  api.subscribeSummaryAvailability.mockImplementationOnce(async (_callback, onError) => {
    onError(new Error('IPC failed'));
    return () => {};
  });
  const target = document.createElement('div');
  const app = mount(SummaryPanel, { target, props: { session, management: false } });
  await vi.waitFor(() => expect(target.textContent).toContain("Couldn't check summary availability."));
  expect(target.textContent).not.toContain('Waiting for the summary model download');
  [...target.querySelectorAll('button')].find((button) => button.textContent === 'Check again')!.click();
  await vi.waitFor(() => expect(target.textContent).not.toContain("Couldn't check summary availability."));
  unmount(app);
});
