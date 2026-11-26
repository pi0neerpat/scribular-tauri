// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { mount, unmount } from 'svelte';
import { accessState } from '../license';

const api = vi.hoisted(() => ({
  listSessions: vi.fn(), countSessions: vi.fn(), searchSessions: vi.fn(), getSession: vi.fn(), getPromptList: vi.fn(), listSummaryModels: vi.fn(),
  onSessionProgress: vi.fn(), cancelJob: vi.fn(), clearAllSessions: vi.fn(), deleteSession: vi.fn(), recoverSession: vi.fn(),
  setShellMenuState: vi.fn().mockResolvedValue(undefined), openSettings: vi.fn().mockResolvedValue(undefined),
  progress: undefined as ((p: unknown) => void) | undefined,
}));
vi.mock('../api', () => ({
  ...api,
  onSessionProgress: (handler: (p: unknown) => void) => { api.progress = handler; return Promise.resolve(() => {}); },
}));
vi.mock('./SummaryPanel.svelte', () => ({ default: () => null }));
const { default: SessionHistory } = await import('./SessionHistory.svelte');
afterEach(() => accessState.set(null));

beforeEach(() => {
  vi.resetAllMocks();
  api.setShellMenuState.mockResolvedValue(undefined);
  api.openSettings.mockResolvedValue(undefined);
  api.getPromptList.mockResolvedValue({ prompts: [], activeId: null, createdId: null });
  api.listSummaryModels.mockResolvedValue([]);
});

const row = (id: string, status = 'done') => ({ id, title: id, createdAt: Date.now(), status, durationMs: null, hasAudio: true, snippet: '', interrupted: false, isSample: false, copiedAt: null as number | null });

it('shows the saved copy marker after reopening and clears it when the list changes', async () => {
  api.listSessions.mockResolvedValue([{ ...row('Copied note'), copiedAt: 123 }]);
  const target = document.createElement('div');
  const first = mount(SessionHistory, { target });
  await vi.waitFor(() => expect(target.querySelector('[aria-label="Copied"]')).not.toBeNull());
  unmount(first);

  const reopened = mount(SessionHistory, { target });
  await vi.waitFor(() => expect(target.querySelector('[aria-label="Copied"]')).not.toBeNull());
  api.listSessions.mockResolvedValue([row('Copied note')]);
  reopened.refreshSessions();
  await vi.waitFor(() => expect(target.querySelector('[aria-label="Copied"]')).toBeNull());
  unmount(reopened);
});

it('refreshes the list when a progress event names an unknown session', async () => {
  api.listSessions.mockResolvedValueOnce([]).mockResolvedValueOnce([row('new', 'failed')]);
  const target = document.createElement('div');
  const app = mount(SessionHistory, { target });
  await vi.waitFor(() => expect(api.listSessions).toHaveBeenCalledTimes(1));
  api.progress?.({ sessionId: 'new', state: 'failed', stepIndex: 1, stepCount: 1, message: 'Done' });
  await vi.waitFor(() => expect(target.textContent).toContain('new'));
  unmount(app);
});

it('reports partial deletion from the native command on the recording view even with a lapsed license', async () => {
  accessState.set({ kind: 'licenseLapsed' });
  api.listSessions.mockResolvedValue([row('one'), row('two')]);
  api.clearAllSessions.mockResolvedValueOnce(null).mockResolvedValueOnce({ deleted: 1, skipped: 1 });
  const target = document.createElement('div');
  const props = $state({ view: 'record' as const, recording: true });
  const app = mount(SessionHistory, { target, props });
  await vi.waitFor(() => expect(target.querySelectorAll('.session')).toHaveLength(2));
  await app.confirmDeleteAll();
  expect(api.clearAllSessions).not.toHaveBeenCalled();
  props.recording = false;
  await app.confirmDeleteAll();
  expect(target.textContent).not.toContain('deleted');
  api.listSessions.mockResolvedValue([row('two')]);
  await app.confirmDeleteAll();
  await vi.waitFor(() => expect(target.querySelectorAll('.session')).toHaveLength(1));
  expect(target.querySelector('aside [role="status"]')?.textContent).toContain('1 deleted; 1 skipped');
  unmount(app);
});

it('keeps Delete All available for real sessions even when search hides them and the license lapses', async () => {
  accessState.set({ kind: 'licenseLapsed' });
  api.listSessions.mockResolvedValue([row('one'), row('two')]);
  api.countSessions.mockResolvedValueOnce(2).mockResolvedValueOnce(0);
  api.searchSessions.mockResolvedValue([]);
  let cancelClear!: (value: null) => void;
  api.clearAllSessions
    .mockReturnValueOnce(new Promise<null>((resolve) => { cancelClear = resolve; }))
    .mockResolvedValueOnce({ deleted: 2, skipped: 0 });
  const target = document.createElement('div');
  const props = $state({ recording: false });
  const app = mount(SessionHistory, { target, props });
  const button = target.querySelector<HTMLButtonElement>('.delete-all')!;
  expect(button.disabled).toBe(true);
  await vi.waitFor(() => expect(button.disabled).toBe(false));
  props.recording = true;
  await vi.waitFor(() => expect(button.disabled).toBe(true));
  button.click();
  expect(api.clearAllSessions).not.toHaveBeenCalled();
  props.recording = false;
  const search = target.querySelector<HTMLInputElement>('input[type="search"]')!;
  search.value = 'no match';
  search.dispatchEvent(new Event('input', { bubbles: true }));
  await vi.waitFor(() => expect(target.querySelectorAll('.session')).toHaveLength(0));
  expect(button.disabled).toBe(false);
  button.click();
  await vi.waitFor(() => expect(button.disabled).toBe(true));
  expect(api.clearAllSessions).toHaveBeenCalledTimes(1);
  cancelClear(null);
  await vi.waitFor(() => expect(button.disabled).toBe(false));
  api.listSessions.mockResolvedValue([]);
  button.click();
  await vi.waitFor(() => expect(target.textContent).toContain('2 deleted; 0 skipped'));
  await vi.waitFor(() => expect(button.disabled).toBe(true));
  search.value = '';
  search.dispatchEvent(new Event('input', { bubbles: true }));
  await vi.waitFor(() => expect(api.listSessions).toHaveBeenCalledTimes(2));
  expect(api.countSessions).toHaveBeenCalledTimes(2);
  expect(target.querySelectorAll('.session')).toHaveLength(0);
  unmount(app);
});

it('shows an empty search result even when checking the total session count fails', async () => {
  api.listSessions.mockResolvedValue([row('one')]);
  api.searchSessions.mockResolvedValue([]);
  api.countSessions.mockRejectedValue(new Error('count unavailable'));
  const target = document.createElement('div');
  const app = mount(SessionHistory, { target });
  await vi.waitFor(() => expect(target.querySelectorAll('.session')).toHaveLength(1));
  const search = target.querySelector<HTMLInputElement>('input[type="search"]')!;
  search.value = 'no match';
  search.dispatchEvent(new Event('input', { bubbles: true }));
  await vi.waitFor(() => expect(target.textContent).toContain('count unavailable'));
  expect(target.querySelectorAll('.session')).toHaveLength(0);
  expect(target.querySelector<HTMLButtonElement>('.delete-all')!.disabled).toBe(false);
  expect(api.listSessions).toHaveBeenCalledTimes(1);
  unmount(app);
});

it('shows row state actions and keeps selected detail when search hides it or fails', async () => {
  const interrupted = { ...row('Interrupted', 'failed'), interrupted: true };
  const queued = row('Queued', 'awaitingModel');
  const complete = row('Complete');
  api.listSessions.mockResolvedValue([interrupted, queued, complete]);
  api.searchSessions.mockResolvedValueOnce([queued]).mockRejectedValueOnce(new Error('offline'));
  api.getSession.mockResolvedValue({ ...complete, summary: 'Saved note' });
  const target = document.createElement('div');
  const app = mount(SessionHistory, { target, props: { onNavigate: (_view: unknown, _opener: unknown, action?: () => void) => action?.() } });
  await vi.waitFor(() => expect(target.textContent).toContain('Complete'));
  expect(target.textContent).toContain('Recover');
  expect(target.textContent).toContain('Writing…');
  const actions = [...target.querySelectorAll('details')].map((node) => node.textContent);
  expect(actions[0]).toContain('Recover');
  expect(actions[1]).toContain('Cancel');
  expect(actions[2]).toContain('Delete');
  (target.querySelectorAll('.session')[2] as HTMLButtonElement).click();
  await vi.waitFor(() => expect(target.querySelector('[aria-current="page"]')?.textContent).toContain('Complete'));
  const input = target.querySelector('input[type="search"]') as HTMLInputElement;
  input.value = 'queued'; input.dispatchEvent(new Event('input', { bubbles: true }));
  await vi.waitFor(() => expect(target.querySelectorAll('.session')).toHaveLength(1));
  expect(target.textContent).toContain('Queued');
  input.value = 'other'; input.dispatchEvent(new Event('input', { bubbles: true }));
  await vi.waitFor(() => expect(target.textContent).toContain('Could not update sidebar'));
  expect(target.querySelectorAll('.session')).toHaveLength(1);
  input.value = ''; input.dispatchEvent(new Event('input', { bubbles: true }));
  await vi.waitFor(() => expect(target.querySelectorAll('.session')).toHaveLength(3));
  expect(target.querySelector('[aria-current="page"]')?.textContent).toContain('Complete');
  unmount(app);
});

it('searches the current input text on each keystroke and restores the full list when cleared', async () => {
  const a = row('Alpha');
  const b = row('Beta');
  api.listSessions.mockResolvedValue([a, b]);
  api.searchSessions.mockImplementation(async (q: string) => [a, b].filter((r) => r.title.toLowerCase().startsWith(q)));
  const target = document.createElement('div');
  const app = mount(SessionHistory, { target });
  await vi.waitFor(() => expect(target.querySelectorAll('.session')).toHaveLength(2));
  const input = target.querySelector('input[type="search"]') as HTMLInputElement;
  const type = (v: string) => { input.value = v; input.dispatchEvent(new Event('input', { bubbles: true })); };
  type('a');
  await vi.waitFor(() => expect(api.searchSessions).toHaveBeenLastCalledWith('a'));
  await vi.waitFor(() => expect(target.querySelectorAll('.session')).toHaveLength(1));
  type('');
  await vi.waitFor(() => expect(target.querySelectorAll('.session')).toHaveLength(2));
  expect(api.searchSessions).toHaveBeenCalledTimes(1);
  unmount(app);
});

it('keeps Copy Note off on the Record page even with a retained history selection (R10)', async () => {
  api.listSessions.mockResolvedValue([row('History')]);
  api.getSession.mockResolvedValue({ ...row('History'), summary: 'Old note' });
  const target = document.createElement('div');
  const app = mount(SessionHistory, { target, props: {
    view: 'record',
    onNavigate: (_view: unknown, _opener: unknown, action?: () => void) => action?.(),
  } });
  await vi.waitFor(() => expect(target.querySelector('.session')).not.toBeNull());
  (target.querySelector('.session') as HTMLButtonElement).click();
  await vi.waitFor(() => expect(api.getSession).toHaveBeenCalledWith('History'));
  await vi.waitFor(() => expect(api.setShellMenuState).toHaveBeenLastCalledWith(expect.objectContaining({ hasNote: false, selected: false })));
  unmount(app);
});

it('opens an interrupted session on its Recover page (R16)', async () => {
  api.listSessions.mockResolvedValue([{ ...row('Lost', 'failed'), interrupted: true }]);
  api.getSession.mockResolvedValue({ ...row('Lost', 'failed'), error: 'interrupted', summary: '', transcript: '' });
  api.recoverSession.mockResolvedValue(undefined);
  const target = document.createElement('div');
  const app = mount(SessionHistory, { target, props: { view: 'history', onNavigate: (_view: unknown, _opener: unknown, action?: () => void) => action?.() } });
  await vi.waitFor(() => expect(target.querySelector('.session')).not.toBeNull());
  (target.querySelector('.session') as HTMLButtonElement).click();
  await vi.waitFor(() => expect(target.querySelector('[aria-label="Recover recording"]')).not.toBeNull());
  expect(target.textContent).toContain('This recording stopped before it finished.');
  [...target.querySelectorAll('button')].find((b) => b.textContent === 'Recover Recording')!.click();
  await vi.waitFor(() => expect(api.recoverSession).toHaveBeenCalledWith('Lost'));
  unmount(app);
});

it('keeps a failed recovery error visible after the list refreshes (R16)', async () => {
  api.listSessions.mockResolvedValue([{ ...row('Lost', 'failed'), interrupted: true }]);
  api.getSession.mockResolvedValue({ ...row('Lost', 'failed'), error: 'interrupted', summary: '', transcript: '' });
  api.recoverSession.mockRejectedValue('no speech found');
  const target = document.createElement('div');
  const app = mount(SessionHistory, { target, props: { view: 'history', onNavigate: (_view: unknown, _opener: unknown, action?: () => void) => action?.() } });
  await vi.waitFor(() => expect(target.querySelector('.session')).not.toBeNull());
  (target.querySelector('.session') as HTMLButtonElement).click();
  await vi.waitFor(() => expect(target.querySelector('[aria-label="Recover recording"]')).not.toBeNull());
  [...target.querySelectorAll('button')].find((b) => b.textContent === 'Recover Recording')!.click();
  await vi.waitFor(() => expect(target.querySelector('[role="alert"]')?.textContent).toContain('no speech found'));
  unmount(app);
});

it('shows the delete confirmation instead of the session page, and clears a vanished session', async () => {
  api.listSessions.mockResolvedValue([{ ...row('Lost', 'failed'), interrupted: true }]);
  api.getSession.mockResolvedValue({ ...row('Lost', 'failed'), error: 'interrupted', summary: '', transcript: '' });
  const target = document.createElement('div');
  const app = mount(SessionHistory, { target, props: { view: 'history', onNavigate: (_view: unknown, _opener: unknown, action?: () => void) => action?.() } });
  await vi.waitFor(() => expect(target.querySelector('.session')).not.toBeNull());
  (target.querySelector('.session') as HTMLButtonElement).click();
  await vi.waitFor(() => expect(target.querySelector('[aria-label="Recover recording"]')).not.toBeNull());
  app.confirmDeleteSelected();
  await vi.waitFor(() => expect(target.querySelector('[role="alertdialog"]')).not.toBeNull());
  expect(target.querySelector('[aria-label="Recover recording"]')).toBeNull();
  api.getSession.mockResolvedValue(null);
  app.openSession('Lost');
  await vi.waitFor(() => expect(target.textContent).toContain('Select a session'));
  unmount(app);
});
