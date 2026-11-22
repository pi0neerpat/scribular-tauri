// @vitest-environment jsdom
import { afterEach, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';
import { fromStore, writable } from 'svelte/store';
import { COACH_CONTEXT, type CoachContext } from '../coach-context';
import type { Session } from '../types/Session';

const api = vi.hoisted(() => ({
  cancelJob: vi.fn(), copyText: vi.fn(), markNoteCopied: vi.fn().mockResolvedValue(undefined), getPromptList: vi.fn(), listSummaryModels: vi.fn(),
  sessionCoachStart: vi.fn(), sessionCoachStop: vi.fn(), sessionCoachAccept: vi.fn(), retrySummaryModel: vi.fn(),
  getSummaryAvailability: vi.fn().mockResolvedValue('ready'), onSummaryAvailability: vi.fn(async () => () => {}),
  subscribeSummaryAvailability: vi.fn(async (callback: (state: string) => void, _onError: (error: unknown) => void) => { callback(await api.getSummaryAvailability()); return () => {}; }),
  onSummaryDelta: vi.fn(), openSettings: vi.fn(), openExternalLink: vi.fn(), regenerateSummary: vi.fn(), undoSummary: vi.fn(), delta: undefined as ((d: unknown) => void) | undefined,
}));
vi.mock('../api', () => ({
  ...api,
  onSummaryDelta: (handler: (d: unknown) => void) => { api.delta = handler; return Promise.resolve(() => {}); },
}));
const { default: SummaryPanel } = await import('./SummaryPanel.svelte');
const { accessState } = await import('../license');

const session: Session = {
  id: 'session-1', title: 'Session', createdAt: 1, inputMode: 'mic', status: 'done', error: null,
  promptId: 'prompt-1', modelId: null, transcript: 'transcript text', plainTranscript: null, rawTranscript: null,
  speakerMap: null, diarizationState: 'none', summary: 'saved summary', durationMs: 1000, timings: null,
  hasAudio: false, audioExpiresAt: null, previousSummary: null, previousPromptId: null, previousModelId: null, canRedo: false, copiedAt: null, isSample: false,
};
const prompts = { prompts: [{ id: 'prompt-1', name: 'Style', description: '', text: '', isDefault: false, showInStyleMenu: true, styleSpec: null }], activeId: 'prompt-1', createdId: null };

afterEach(() => accessState.set(null));

it('restores the saved copy marker and clears it when the note changes', () => {
  api.listSummaryModels.mockResolvedValue([]);
  const current = writable<Session>({ ...session, copiedAt: 123 });
  const state = fromStore(current);
  const target = document.createElement('div');
  const app = mount(SummaryPanel, { target, props: { get session() { return state.current; }, management: true, prompts, models: [] } });
  const copyButton = () => [...target.querySelectorAll('button')].find((button) => button.textContent === 'Copied' || button.textContent === 'Copy Note');
  flushSync();
  expect(copyButton()?.textContent).toBe('Copied');
  current.set({ ...session, summary: 'Revised note', copiedAt: null });
  flushSync();
  expect(copyButton()?.textContent).toBe('Copy Note');
  unmount(app);
});

it('keeps copy and delete available but disables regeneration for a lapsed licence', async () => {
  accessState.set({ kind: 'licenseLapsed' });
  api.listSummaryModels.mockResolvedValue([]);
  const target = document.createElement('div');
  const app = mount(SummaryPanel, { target, props: { session, management: true, prompts, models: [], onDelete: vi.fn() } });
  await vi.waitFor(() => expect(target.textContent).toContain('Your existing notes are still available.'));
  const named = (label: string) => [...target.querySelectorAll('button')].find((b) => b.textContent === label)!;
  expect(named('Copy Note').disabled).toBe(false);
  expect(named('Regenerate').disabled).toBe(true);
  expect(named('Delete').disabled).toBe(false);
  unmount(app);
  accessState.set(null);
});

it('copies the saved summary, reports success, and keeps streamed regeneration text in preview', async () => {
  const writeText = vi.fn().mockResolvedValue(undefined);
  api.listSummaryModels.mockResolvedValue([]);
  Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText } });
  const target = document.createElement('div');
  const app = mount(SummaryPanel, { target, props: { session, management: true, prompts, models: [] } });
  target.querySelector('button')!.click();
  await vi.waitFor(() => expect(writeText).toHaveBeenCalledWith('saved summary'));
  await vi.waitFor(() => expect(api.markNoteCopied).toHaveBeenCalledWith('session-1', 'saved summary'));
  await vi.waitFor(() => expect(target.textContent).toContain('Copied'));
  target.querySelectorAll('button')[1].click();
  await vi.waitFor(() => expect(api.regenerateSummary).toHaveBeenCalledWith('session-1', 'prompt-1'));
  api.delta?.({ sessionId: 'session-1', text: 'new preview' });
  flushSync();
  expect(target.textContent).toContain('saved summary');
  expect(target.textContent).toContain('new preview');
  unmount(app);
});

it('does not mark a failed clipboard copy as copied', async () => {
  const writeText = vi.fn().mockRejectedValue(new Error('clipboard denied'));
  api.markNoteCopied.mockClear();
  Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText } });
  const target = document.createElement('div');
  const app = mount(SummaryPanel, { target, props: { session, management: true, prompts, models: [] } });
  await app.copy();
  expect(api.markNoteCopied).not.toHaveBeenCalled();
  expect(target.textContent).toContain('clipboard denied');
  expect(target.textContent).not.toContain('Copied');
  unmount(app);
});

it('copies the saved note while a candidate stays pending', async () => {
  let result!: (value: unknown) => void;
  api.sessionCoachStart.mockImplementation(async (_id, _request, onResult) => {
    result = onResult;
    return { jobId: 'job', unlisten: vi.fn() };
  });
  const writeText = vi.fn().mockResolvedValue(undefined);
  Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText } });
  const coach: CoachContext = {
    availability: () => ({ reason: null, message: null, sessionMessage: null, retry: null }),
    open: vi.fn(), firstNoteTip: () => false, dismissTip: vi.fn(), styles: () => prompts, updateStyles: vi.fn(),
  };
  const target = document.createElement('div');
  const app = mount(SummaryPanel, { target, props: { session, management: true, prompts, models: [] }, context: new Map([[COACH_CONTEXT, coach]]) });
  const input = target.querySelector('textarea')!;
  input.value = 'Shorten Plan';
  input.dispatchEvent(new Event('input', { bubbles: true }));
  target.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
  await vi.waitFor(() => expect(result).toBeDefined());
  result({ jobId: 'job', sessionId: 'session-1', outcome: { kind: 'candidate', candidateId: 'candidate', note: 'Revised note' } });
  flushSync();
  await app.copy();
  expect(writeText).toHaveBeenCalledWith('saved summary');
  expect(target.textContent).toContain('Revised note');
  expect(target.textContent).toContain('Use This');
  unmount(app);
});

it('streams regeneration on the record panel after the first summary arrives in place', async () => {
  api.delta = undefined;
  api.regenerateSummary.mockClear().mockResolvedValue(undefined);
  const current = writable<Session>({ ...session, status: 'awaitingModel', summary: '' });
  const state = fromStore(current);
  const coach: CoachContext = {
    availability: () => ({ reason: null, message: null, sessionMessage: null, retry: null }),
    open: vi.fn(), firstNoteTip: () => false, dismissTip: vi.fn(), styles: () => prompts, updateStyles: vi.fn(),
  };
  const target = document.createElement('div');
  const app = mount(SummaryPanel, { target, props: { get session() { return state.current; } }, context: new Map([[COACH_CONTEXT, coach]]) });
  const named = (label: string) => [...target.querySelectorAll('button')].find((b) => b.textContent === label);
  try {
    expect(named('Regenerate')).toBeUndefined();
    // The completed first summary replaces the pending session without remounting the panel.
    const delta = (sessionId: string, text: string) => api.delta?.({ sessionId, text });
    delta(session.id, 'first pass');
    current.set(session);
    await vi.waitFor(() => expect(named('Regenerate')?.disabled).toBe(false));
    expect(target.textContent).not.toContain('New summary preview');
    named('Regenerate')!.click();
    await vi.waitFor(() => expect(api.regenerateSummary).toHaveBeenCalledWith(session.id, 'prompt-1'));
    current.set({ ...session, status: 'summarizing' });
    delta('other-session', 'unrelated delta');
    delta(session.id, 'regeneration delta');
    flushSync();
    expect(named('Stop')).toBeDefined();
    expect(target.textContent).toContain('New summary preview');
    expect(target.textContent).toContain('regeneration delta');
    expect(target.textContent).not.toContain('unrelated delta');
    current.set({ ...session, summary: 'regenerated summary' });
    flushSync();
    expect(target.textContent).toContain('regenerated summary');
    expect(target.textContent).not.toContain('New summary preview');
  } finally {
    unmount(app);
  }
});

it('uses the panel copy action and delete confirmation for menu requests', async () => {
  const writeText = vi.fn().mockResolvedValue(undefined);
  const onDelete = vi.fn();
  api.listSummaryModels.mockResolvedValue([]);
  Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText } });
  const target = document.createElement('div');
  const app = mount(SummaryPanel, { target, props: { session, management: true, prompts, models: [], onDelete } });
  await app.copy();
  expect(writeText).toHaveBeenCalledWith('saved summary');
  app.requestDelete();
  flushSync();
  expect(target.textContent).toContain('Confirm');
  [...target.querySelectorAll('button')].find((button) => button.textContent === 'Confirm')!.click();
  expect(onDelete).toHaveBeenCalledOnce();
  unmount(app);
});

it('disables Copy when both summary and transcript are empty', () => {
  api.listSummaryModels.mockResolvedValue([]);
  const target = document.createElement('div');
  const app = mount(SummaryPanel, { target, props: {
    session: { ...session, summary: '', transcript: '' }, management: true, prompts, models: [],
  } });
  expect([...target.querySelectorAll('button')].find((b) => b.textContent === 'Copy Note')?.disabled).toBe(true);
  unmount(app);
});

it('does not copy a transcript as a note when the saved note is empty', async () => {
  api.listSummaryModels.mockResolvedValue([]);
  const writeText = vi.fn().mockResolvedValue(undefined);
  Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText } });
  const target = document.createElement('div');
  const app = mount(SummaryPanel, { target, props: {
    session: { ...session, summary: '' }, management: true, prompts, models: [],
  } });
  expect([...target.querySelectorAll('button')].find((b) => b.textContent === 'Copy Note')?.disabled).toBe(true);
  expect(writeText).not.toHaveBeenCalled();
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
