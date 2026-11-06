// @vitest-environment jsdom
import { expect, it, vi } from 'vitest';
import { mount, unmount } from 'svelte';

const api = vi.hoisted(() => ({
  listSessions: vi.fn(), getSession: vi.fn(), getPromptList: vi.fn(), listSummaryModels: vi.fn(),
  onSessionProgress: vi.fn(), cancelJob: vi.fn(), clearAllSessions: vi.fn(), deleteSession: vi.fn(), recoverSession: vi.fn(),
  progress: undefined as ((p: unknown) => void) | undefined,
}));
vi.mock('../api', () => ({
  ...api,
  onSessionProgress: (handler: (p: unknown) => void) => { api.progress = handler; return Promise.resolve(() => {}); },
}));
vi.mock('./SummaryPanel.svelte', () => ({ default: () => null }));
const { default: SessionHistory } = await import('./SessionHistory.svelte');

it('refreshes the list when a progress event names an unknown session', async () => {
  const row = { id: 'new', title: 'New session', createdAt: 1, status: 'failed', durationMs: null, hasAudio: false, snippet: 'No summary available', interrupted: false };
  api.listSessions.mockResolvedValueOnce([]).mockResolvedValueOnce([row]);
  api.getPromptList.mockResolvedValue({ prompts: [], activeId: null, createdId: null });
  api.listSummaryModels.mockResolvedValue([]);
  const target = document.createElement('div');
  const app = mount(SessionHistory, { target });
  await vi.waitFor(() => expect(target.textContent).toContain('0 sessions'));
  api.progress?.({ sessionId: 'new', state: 'failed', stepIndex: 1, stepCount: 1, message: 'Done' });
  await vi.waitFor(() => expect(target.textContent).toContain('1 session'));
  expect(target.textContent).toContain('New session');
  unmount(app);
});
