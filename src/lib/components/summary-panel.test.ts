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
  onSummaryDelta: vi.fn(), openSettings: vi.fn(), openExternalLink: vi.fn(), regenerateSummary: vi.fn(), undoSummary: vi.fn(), redoSummary: vi.fn(), updateSession: vi.fn(), getSession: vi.fn(), delta: undefined as ((d: unknown) => void) | undefined,
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
const named = (target: HTMLElement, label: string) => [...target.querySelectorAll('button')].find((b) => b.textContent === label);
const noteBox = (target: HTMLElement) => target.querySelector<HTMLTextAreaElement>('textarea[aria-label="Note"]');
const styleSelect = (target: HTMLElement) => target.querySelector<HTMLSelectElement>('select')!;
function pick(target: HTMLElement, id: string) {
  const select = styleSelect(target);
  select.value = id;
  select.dispatchEvent(new Event('change', { bubbles: true }));
}

it('shows Copied only after a copy in this view, never from the saved marker (R13)', () => {
  api.listSummaryModels.mockResolvedValue([]);
  const target = document.createElement('div');
  const app = mount(SummaryPanel, { target, props: { session: { ...session, copiedAt: 123 }, management: true, prompts, models: [] } });
  flushSync();
  expect(named(target, 'Copy Note')).toBeDefined();
  expect(named(target, 'Copied')).toBeUndefined();
  unmount(app);
});

it('keeps view and copy but blocks edits and rewrites for a lapsed licence', async () => {
  accessState.set({ kind: 'licenseLapsed' });
  api.listSummaryModels.mockResolvedValue([]);
  const target = document.createElement('div');
  const app = mount(SummaryPanel, { target, props: { session, management: true, prompts, models: [] } });
  await vi.waitFor(() => expect(target.textContent).toContain('Your existing notes are still available.'));
  expect(named(target, 'Copy Note')!.disabled).toBe(false);
  expect(styleSelect(target).disabled).toBe(true);
  expect(noteBox(target)).toBeNull();
  expect(target.querySelector('pre[aria-label="Note"]')?.textContent).toBe('saved summary');
  expect(named(target, 'Regenerate')).toBeUndefined();
  expect(named(target, 'Delete')).toBeUndefined();
  unmount(app);
  accessState.set(null);
});

it('copies the saved summary, reports success, and rewrites when a style is picked (R12)', async () => {
  const writeText = vi.fn().mockResolvedValue(undefined);
  api.listSummaryModels.mockResolvedValue([]);
  api.regenerateSummary.mockClear().mockResolvedValue(undefined);
  Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText } });
  const target = document.createElement('div');
  const app = mount(SummaryPanel, { target, props: { session, management: true, prompts, models: [] } });
  named(target, 'Copy Note')!.click();
  await vi.waitFor(() => expect(writeText).toHaveBeenCalledWith('saved summary'));
  await vi.waitFor(() => expect(api.markNoteCopied).toHaveBeenCalledWith('session-1', 'saved summary'));
  await vi.waitFor(() => expect(named(target, 'Copied')).toBeDefined());
  await vi.waitFor(() => expect(styleSelect(target).disabled).toBe(false));
  pick(target, 'prompt-1');
  await vi.waitFor(() => expect(api.regenerateSummary).toHaveBeenCalledWith('session-1', 'prompt-1'));
  expect(named(target, 'Regenerate')).toBeUndefined();
  unmount(app);
});

it('saves an in-place edit on blur and before a rewrite (R12)', async () => {
  api.listSummaryModels.mockResolvedValue([]);
  api.updateSession.mockClear().mockImplementation(async (_id: string, patch: { summary: string }) => ({ ...session, summary: patch.summary }));
  api.regenerateSummary.mockClear().mockResolvedValue(undefined);
  const onUpdate = vi.fn();
  const target = document.createElement('div');
  const app = mount(SummaryPanel, { target, props: { session, management: true, prompts, models: [], onUpdate } });
  const box = noteBox(target)!;
  box.value = 'my edit';
  box.dispatchEvent(new Event('input', { bubbles: true }));
  box.dispatchEvent(new Event('blur'));
  await vi.waitFor(() => expect(api.updateSession).toHaveBeenCalledWith('session-1', { summary: 'my edit' }));
  await vi.waitFor(() => expect(onUpdate).toHaveBeenCalledWith(expect.objectContaining({ summary: 'my edit' })));
  box.value = 'second edit';
  box.dispatchEvent(new Event('input', { bubbles: true }));
  await vi.waitFor(() => expect(styleSelect(target).disabled).toBe(false));
  pick(target, 'prompt-1');
  await vi.waitFor(() => expect(api.regenerateSummary).toHaveBeenCalled());
  expect(api.updateSession).toHaveBeenLastCalledWith('session-1', { summary: 'second edit' });
  expect(api.updateSession.mock.invocationCallOrder.at(-1)!).toBeLessThan(api.regenerateSummary.mock.invocationCallOrder[0]);
  unmount(app);
});

it('shows Undo after a rewrite finishes and clears it on copy (R12)', async () => {
  api.listSummaryModels.mockResolvedValue([]);
  api.regenerateSummary.mockClear().mockResolvedValue(undefined);
  const rewritten: Session = { ...session, summary: 'rewritten', previousSummary: 'saved summary' };
  api.undoSummary.mockClear().mockResolvedValue({ ...session, canRedo: true });
  Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText: vi.fn().mockResolvedValue(undefined) } });
  const current = writable<Session>(session);
  const state = fromStore(current);
  const onUpdate = vi.fn((next: Session) => current.set(next));
  const target = document.createElement('div');
  const app = mount(SummaryPanel, { target, props: { get session() { return state.current; }, management: true, prompts, models: [], onUpdate } });
  await vi.waitFor(() => expect(styleSelect(target).disabled).toBe(false));
  pick(target, 'prompt-1');
  await vi.waitFor(() => expect(api.regenerateSummary).toHaveBeenCalled());
  current.set({ ...session, status: 'summarizing' });
  flushSync();
  expect(named(target, 'Stop')).toBeDefined();
  current.set(rewritten);
  await vi.waitFor(() => expect(named(target, 'Undo')).toBeDefined());
  expect(noteBox(target)!.value).toBe('rewritten');
  await app.copy();
  await vi.waitFor(() => expect(named(target, 'Undo')).toBeUndefined());
  expect(await app.undo()).toBe(true);
  expect(api.undoSummary).toHaveBeenCalledWith('session-1');
  unmount(app);
});

it('opens on Note, copies the transcript without marking the note, and disables Audio without kept audio (R11, R13, R15)', async () => {
  api.listSummaryModels.mockResolvedValue([]);
  api.markNoteCopied.mockClear();
  const writeText = vi.fn().mockResolvedValue(undefined);
  Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText } });
  const target = document.createElement('div');
  const app = mount(SummaryPanel, { target, props: { session, management: true, prompts, models: [] } });
  flushSync();
  const tabs = [...target.querySelectorAll<HTMLButtonElement>('[role="tab"]')];
  expect(tabs.map((t) => t.textContent)).toEqual(['Note', 'Transcript', 'Audio']);
  expect(tabs[0].getAttribute('aria-selected')).toBe('true');
  expect(tabs[2].disabled).toBe(true);
  expect(tabs[2].title).toContain('No audio was kept');
  app.showTab('transcript');
  flushSync();
  expect(target.querySelector('[role="tabpanel"]')?.getAttribute('aria-label')).toBe('Transcript');
  named(target, 'Copy Transcript')!.click();
  await vi.waitFor(() => expect(writeText).toHaveBeenCalledWith('transcript text'));
  expect(api.markNoteCopied).not.toHaveBeenCalled();
  app.showTab('audio');
  flushSync();
  expect(target.querySelector('[role="tabpanel"]')?.getAttribute('aria-label')).toBe('Transcript');
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
    open: vi.fn(), firstNoteTip: () => false, tipSaving: () => false, tipSaveFailed: () => false, dismissTip: vi.fn(), styles: () => prompts, updateStyles: vi.fn(),
  };
  const target = document.createElement('div');
  const app = mount(SummaryPanel, { target, props: { session, management: true, prompts, models: [] }, context: new Map([[COACH_CONTEXT, coach]]) });
  const input = target.querySelector<HTMLTextAreaElement>('#coach-request')!;
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

it('streams a rewrite after the first summary arrives in place', async () => {
  api.delta = undefined;
  api.regenerateSummary.mockClear().mockResolvedValue(undefined);
  const current = writable<Session>({ ...session, status: 'awaitingModel', summary: '' });
  const state = fromStore(current);
  const coach: CoachContext = {
    availability: () => ({ reason: null, message: null, sessionMessage: null, retry: null }),
    open: vi.fn(), firstNoteTip: () => false, tipSaving: () => false, tipSaveFailed: () => false, dismissTip: vi.fn(), styles: () => prompts, updateStyles: vi.fn(),
  };
  const target = document.createElement('div');
  const app = mount(SummaryPanel, { target, props: { get session() { return state.current; } }, context: new Map([[COACH_CONTEXT, coach]]) });
  flushSync();
  try {
    expect(named(target, 'Cancel')).toBeDefined();
    expect(named(target, 'Style')?.disabled).toBe(true);
    // The completed first summary replaces the pending session without remounting the panel.
    const delta = (sessionId: string, text: string) => api.delta?.({ sessionId, text });
    delta(session.id, 'first pass');
    current.set(session);
    await vi.waitFor(() => expect(named(target, 'Style')?.disabled).toBe(false));
    expect(target.querySelector('[aria-label="Note in progress"]')).toBeNull();
    named(target, 'Style')!.click();
    await vi.waitFor(() => expect(api.regenerateSummary).toHaveBeenCalledWith(session.id, 'prompt-1'));
    current.set({ ...session, status: 'summarizing' });
    delta('other-session', 'unrelated delta');
    delta(session.id, 'regeneration delta');
    flushSync();
    expect(named(target, 'Stop')).toBeDefined();
    expect(named(target, 'Copy Note')?.disabled).toBe(true);
    expect(target.querySelector('[aria-label="Note in progress"]')?.textContent).toBe('regeneration delta');
    current.set({ ...session, summary: 'regenerated summary' });
    flushSync();
    expect(noteBox(target)?.value).toBe('regenerated summary');
    expect(target.querySelector('[aria-label="Note in progress"]')).toBeNull();
  } finally {
    unmount(app);
  }
});

it('uses the panel copy action for menu requests', async () => {
  const writeText = vi.fn().mockResolvedValue(undefined);
  api.listSummaryModels.mockResolvedValue([]);
  Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText } });
  const target = document.createElement('div');
  const app = mount(SummaryPanel, { target, props: { session, management: true, prompts, models: [] } });
  await app.copy();
  expect(writeText).toHaveBeenCalledWith('saved summary');
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

it('keeps an existing summary visible, offers Try Again and disables rewrites when summaries fail to load', async () => {
  api.getSummaryAvailability.mockResolvedValueOnce('loadFailed');
  api.listSummaryModels.mockResolvedValue([]);
  api.retrySummaryModel.mockClear().mockResolvedValue(undefined);
  const target = document.createElement('div');
  const app = mount(SummaryPanel, { target, props: { session, management: true, prompts, models: [] } });
  await vi.waitFor(() => expect(target.textContent).toContain("Summaries couldn't load."));
  expect(noteBox(target)?.value).toBe('saved summary');
  expect(styleSelect(target).disabled).toBe(true);
  named(target, 'Try Again')!.click();
  expect(api.retrySummaryModel).toHaveBeenCalledOnce();
  unmount(app);
});

it('explains unsupported Macs without a retry', async () => {
  api.getSummaryAvailability.mockResolvedValueOnce('unsupported');
  api.listSummaryModels.mockResolvedValue([]);
  const target = document.createElement('div');
  const app = mount(SummaryPanel, { target, props: { session, management: true, prompts, models: [] } });
  await vi.waitFor(() => expect(target.textContent).toContain("Summaries aren't available on this Mac."));
  expect(named(target, 'Try Again')).toBeUndefined();
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

it('copies nothing when saving an unsaved edit fails (R13)', async () => {
  const writeText = vi.fn().mockResolvedValue(undefined);
  Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText } });
  api.listSummaryModels.mockResolvedValue([]);
  api.markNoteCopied.mockClear();
  api.updateSession.mockReset().mockRejectedValue('disk full');
  const target = document.createElement('div');
  const app = mount(SummaryPanel, { target, props: { session, management: true, prompts, models: [] } });
  flushSync();
  const box = noteBox(target)!;
  box.value = 'unsaved edit';
  box.dispatchEvent(new Event('input', { bubbles: true }));
  flushSync();
  await app.copy();
  expect(writeText).not.toHaveBeenCalled();
  expect(api.markNoteCopied).not.toHaveBeenCalled();
  expect(target.textContent).toContain('disk full');
  unmount(app);
});

it('runs Undo only after an in-flight save of the draft finishes (R12)', async () => {
  let finishSave!: () => void;
  const rewritten = { ...session, previousSummary: 'older note' };
  api.updateSession.mockReset().mockImplementation((_id: string, patch: { summary: string }) =>
    new Promise((resolve) => { finishSave = () => resolve({ ...rewritten, summary: patch.summary }); }));
  api.undoSummary.mockReset().mockResolvedValue({ ...session, summary: 'older note', canRedo: true });
  api.listSummaryModels.mockResolvedValue([]);
  const target = document.createElement('div');
  const app = mount(SummaryPanel, { target, props: { session: rewritten, management: true, prompts, models: [] } });
  const box = noteBox(target)!;
  box.value = 'typed';
  box.dispatchEvent(new Event('input', { bubbles: true }));
  box.dispatchEvent(new Event('blur'));
  const undone = app.undo();
  await Promise.resolve();
  expect(api.undoSummary).not.toHaveBeenCalled();
  finishSave();
  expect(await undone).toBe(true);
  expect(api.updateSession).toHaveBeenCalledTimes(1);
  expect(api.updateSession.mock.invocationCallOrder[0]).toBeLessThan(api.undoSummary.mock.invocationCallOrder[0]);
  unmount(app);
});
