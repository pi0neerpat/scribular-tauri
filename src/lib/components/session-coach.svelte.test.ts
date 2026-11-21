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

afterEach(() => { api.sessionCoachStart.mockReset(); api.sessionCoachAccept.mockReset(); });

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
  await vi.waitFor(() => expect(ui.button('Stop')).toBeDefined());
  expect(ui.button('Use This').disabled).toBe(true);
  ui.target.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
  flushSync();
  expect(ui.button('Use This').disabled).toBe(false);
  callbacks[1]({ jobId: 'job-2', sessionId: 'one', outcome: { kind: 'candidate', candidateId: 'b', note: 'Late note' } });
  flushSync();
  expect(ui.target.textContent).not.toContain('Late note');
  expect(ui.state.session.summary).toBe('Saved note');
  unmount(ui.app);
});

it('restores the earlier candidate when a follow-up fails', async () => {
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
  expect(ui.target.querySelector('textarea')?.disabled).toBe(true);
  expect(ui.target.textContent).toContain('Summaries could not load');
  expect(ui.button('Try Again')).toBeDefined();
  unmount(ui.app);
});
