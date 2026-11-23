// @vitest-environment jsdom
import { afterEach, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';
import type { Session } from '../types/Session';
import type { CoachAvailability } from '../types/CoachAvailability';

const api = vi.hoisted(() => ({
  sessionCoachStart: vi.fn(), sessionCoachStop: vi.fn().mockResolvedValue(undefined),
  sessionCoachAccept: vi.fn(), retrySummaryModel: vi.fn(),
}));
vi.mock('../api', () => api);
const { default: SessionCoach } = await import('./SessionCoach.svelte');
const saved = { id: 'one', summary: 'Saved note', status: 'done' } as Session;
const ready = { reason: null, message: null, sessionMessage: null, retry: null };

function setup() {
  const target = document.createElement('div');
  const state = $state<{ session: Session; availability: CoachAvailability }>({ session: saved, availability: ready });
  const app = mount(SessionCoach, { target, props: { get session() { return state.session; }, get availability() { return state.availability; } } });
  const button = (label: string) => [...target.querySelectorAll('button')].find((item) => item.textContent === label)!;
  const send = (request: string) => {
    const input = target.querySelector('textarea')!;
    input.value = request;
    input.dispatchEvent(new Event('input', { bubbles: true }));
    flushSync();
    target.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
  };
  return { target, state, app, button, send };
}

afterEach(() => {
  api.sessionCoachStart.mockReset();
  api.sessionCoachAccept.mockReset();
  api.sessionCoachStop.mockReset().mockResolvedValue(undefined);
});

it('keeps the saved note, replaces a candidate after follow-up, and drops chat on leave', async () => {
  const callbacks: ((result: unknown) => void)[] = [];
  api.sessionCoachStart.mockImplementation(async (_id, _request, onResult) => {
    callbacks.push(onResult);
    return { jobId: `job-${callbacks.length}`, unlisten: vi.fn() };
  });
  const ui = setup();
  ui.send('Shorten Plan');
  await vi.waitFor(() => expect(ui.button('Stop')).toBeDefined());
  callbacks[0]({ jobId: 'job-1', sessionId: 'one', outcome: { kind: 'candidate', candidateId: 'a', note: 'First note' } });
  flushSync();
  expect(ui.target.textContent).toContain('First note');
  ui.button('Keep the Old One').click();
  ui.send('Use bullet points');
  await vi.waitFor(() => expect(callbacks).toHaveLength(2));
  callbacks[1]({ jobId: 'job-2', sessionId: 'one', outcome: { kind: 'candidate', candidateId: 'b', note: 'Second note' } });
  flushSync();
  expect(ui.target.textContent).toContain('Second note');
  expect(ui.target.textContent).not.toContain('First note');
  expect(ui.state.session.summary).toBe('Saved note');
  unmount(ui.app);
  const fresh = setup();
  expect(fresh.target.textContent).not.toContain('Second note');
  unmount(fresh.app);
});

it('disables candidate actions during follow-up and ignores a stopped late result', async () => {
  const callbacks: ((result: unknown) => void)[] = [];
  let finishStop!: () => void;
  api.sessionCoachStop.mockReturnValue(new Promise<void>((resolve) => { finishStop = resolve; }));
  api.sessionCoachAccept.mockResolvedValue({ ...saved, summary: 'First note' });
  api.sessionCoachStart.mockImplementation(async (_id, _request, onResult) => {
    callbacks.push(onResult);
    return { jobId: `job-${callbacks.length}`, unlisten: vi.fn() };
  });
  const ui = setup();
  ui.send('Shorten Plan');
  await vi.waitFor(() => expect(ui.button('Stop')).toBeDefined());
  callbacks[0]({ jobId: 'job-1', sessionId: 'one', outcome: { kind: 'candidate', candidateId: 'a', note: 'First note' } });
  flushSync();
  ui.send('Add bullets');
  await vi.waitFor(() => expect(callbacks).toHaveLength(2));
  expect(ui.button('Use This').disabled).toBe(true);
  ui.target.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
  flushSync();
  expect(ui.button('Use This').disabled).toBe(true);
  callbacks[1]({ jobId: 'job-2', sessionId: 'one', outcome: { kind: 'candidate', candidateId: 'b', note: 'Late note' } });
  flushSync();
  expect(ui.target.textContent).not.toContain('Late note');
  expect(ui.state.session.summary).toBe('Saved note');
  expect(api.sessionCoachStop).toHaveBeenCalledWith('job-2');
  finishStop();
  await vi.waitFor(() => expect(ui.button('Use This').disabled).toBe(false));
  ui.button('Use This').click();
  expect(api.sessionCoachAccept).toHaveBeenCalledWith('one', 'a');
  await vi.waitFor(() => expect(ui.target.textContent).toContain('Revision saved.'));
  unmount(ui.app);
});

it('restores a candidate after failure and compares a successful follow-up to the saved note', async () => {
  const callbacks: ((result: unknown) => void)[] = [];
  api.sessionCoachStart.mockImplementation(async (_id, _request, onResult) => {
    callbacks.push(onResult);
    return { jobId: `job-${callbacks.length}`, unlisten: vi.fn() };
  });
  const ui = setup();
  ui.send('Shorten Plan');
  await vi.waitFor(() => expect(ui.button('Stop')).toBeDefined());
  callbacks[0]({ jobId: 'job-1', sessionId: 'one', outcome: { kind: 'candidate', candidateId: 'a', note: 'First note' } });
  flushSync();
  ui.send('Add bullets');
  await vi.waitFor(() => expect(ui.button('Use This').disabled).toBe(true));
  callbacks[1]({ jobId: 'job-2', sessionId: 'one', outcome: { kind: 'reply', message: 'Try again.' } });
  flushSync();
  expect(ui.button('Use This').disabled).toBe(false);
  expect(ui.target.textContent).toContain('First note');
  expect(ui.target.textContent).toContain('Try again.');
  expect(ui.state.session.summary).toBe('Saved note');

  ui.send('Use bullet points');
  await vi.waitFor(() => expect(callbacks).toHaveLength(3));
  expect(api.sessionCoachStart.mock.lastCall?.slice(4)).toEqual([['Shorten Plan'], 'a']);
  callbacks[2]({ jobId: 'job-3', sessionId: 'one', outcome: { kind: 'candidate', candidateId: 'b', note: 'Combined note' } });
  flushSync();
  expect(ui.target.querySelector('del')?.textContent).toBe('Saved note');
  expect(ui.target.querySelector('ins')?.textContent).toBe('Combined note');
  expect(ui.button('Use This').disabled).toBe(false);
  unmount(ui.app);
});

it('shows stale acceptance and the shared availability reason', async () => {
  let result!: (result: unknown) => void;
  api.sessionCoachStart.mockImplementation(async (_id, _request, onResult) => {
    result = onResult;
    return { jobId: 'job', unlisten: vi.fn() };
  });
  api.sessionCoachAccept.mockRejectedValue('The note changed. Ask for a new revision to review.');
  const ui = setup();
  ui.send('Shorten Plan');
  await vi.waitFor(() => expect(ui.button('Stop'), ui.target.innerHTML).toBeDefined());
  result({ jobId: 'job', sessionId: 'one', outcome: { kind: 'candidate', candidateId: 'a', note: 'New note' } });
  flushSync();
  ui.button('Use This').click();
  await vi.waitFor(() => expect(ui.target.textContent).toContain('note changed'));
  expect(ui.target.textContent).not.toContain('New note');
  ui.state.availability = { reason: 'loadFailed', message: 'Failed', sessionMessage: 'Summaries could not load', retry: 'tryAgain' };
  flushSync();
  const input = ui.target.querySelector('textarea')!;
  expect(input.disabled).toBe(true);
  expect(input.placeholder).toBe('Summaries could not load');
  expect(ui.target.querySelector(`#${input.getAttribute('aria-describedby')}`)?.textContent).toBe(input.placeholder);
  expect(ui.button('Try Again')).toBeDefined();
  unmount(ui.app);
});

it('stops a job whose start response arrives after Stop', async () => {
  let finish!: (value: { jobId: string; unlisten: () => void }) => void;
  const unlisten = vi.fn();
  api.sessionCoachStart.mockReturnValue(new Promise((resolve) => { finish = resolve; }));
  const ui = setup();
  ui.send('Shorten Plan');
  flushSync();
  ui.target.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
  finish({ jobId: 'late-job', unlisten });
  await vi.waitFor(() => expect(api.sessionCoachStop).toHaveBeenCalledWith('late-job'));
  expect(unlisten).toHaveBeenCalledOnce();
  unmount(ui.app);
});

it('keeps candidate actions disabled while acceptance is pending', async () => {
  let result!: (value: unknown) => void;
  let finish!: (value: Session) => void;
  api.sessionCoachStart.mockImplementation(async (_id, _request, onResult) => {
    result = onResult;
    return { jobId: 'job', unlisten: vi.fn() };
  });
  api.sessionCoachAccept.mockReturnValue(new Promise((resolve) => { finish = resolve; }));
  const ui = setup();
  ui.send('Shorten Plan');
  await vi.waitFor(() => expect(result).toBeDefined());
  result({ jobId: 'job', sessionId: 'one', outcome: { kind: 'candidate', candidateId: 'a', note: 'New note' } });
  flushSync();
  ui.button('Use This').click();
  flushSync();
  expect(ui.button('Use This').disabled).toBe(true);
  expect(ui.button('Keep the Old One').disabled).toBe(true);
  finish({ ...saved, summary: 'New note' });
  await vi.waitFor(() => expect(ui.target.textContent).toContain('Revision saved.'));
  unmount(ui.app);
});

it('shows moved and repeated lines in their actual order', async () => {
  let result!: (value: unknown) => void;
  api.sessionCoachStart.mockImplementation(async (_id, _request, onResult) => {
    result = onResult;
    return { jobId: 'job', unlisten: vi.fn() };
  });
  const ui = setup();
  ui.state.session = { ...saved, summary: 'A\nB\nA' };
  ui.send('Reorder');
  await vi.waitFor(() => expect(result).toBeDefined());
  result({ jobId: 'job', sessionId: 'one', outcome: { kind: 'candidate', candidateId: 'a', note: 'A\nA\nB' } });
  flushSync();
  const rows = [...ui.target.querySelectorAll('[aria-label="Note changes"] > *')];
  expect(rows.map((row) => [row.tagName, row.textContent])).toEqual([
    ['SPAN', 'A'], ['DEL', 'B'], ['SPAN', 'A'], ['INS', 'B'],
  ]);
  unmount(ui.app);
});
