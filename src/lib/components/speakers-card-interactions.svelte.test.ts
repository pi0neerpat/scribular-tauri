// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';
import type { Session } from '../types/Session';

const confirmSpeakerLabels = vi.fn();
const getSession = vi.fn();
const skipDiarization = vi.fn();
const updateSpeakerMap = vi.fn();
vi.mock('../api', () => ({ confirmSpeakerLabels, getSession, skipDiarization, updateSpeakerMap }));
const { default: SpeakersCard } = await import('./SpeakersCard.svelte');
const { default: SummaryPanel } = await import('./SummaryPanel.svelte');

function session(state: Session['diarizationState'], name = 'Alice', summary = 'Alice spoke') {
  return {
    id: 'session-1', title: 'Test', createdAt: 0, inputMode: 'mic', status: 'done', error: null,
    promptId: null, modelId: null, transcript: 'Alice: hello', plainTranscript: 'hello',
    rawTranscript: 'SPEAKER_0: hello', speakerMap: { SPEAKER_0: name }, diarizationState: state,
    summary, durationMs: 1000, timings: null, hasAudio: false, isSample: false, audioExpiresAt: null,
    previousSummary: null, previousPromptId: null, previousModelId: null,
  } as Session;
}

afterEach(() => vi.clearAllMocks());

it('Add speakers after Skip confirms labels and refreshes the card to confirmed', async () => {
  const confirmed = session('confirmed');
  getSession.mockResolvedValue(confirmed);
  confirmSpeakerLabels.mockResolvedValue('Alice: hello');
  const target = document.createElement('div');
  const app = mount(SpeakersCard, { target, props: { session: session('skipped') } });
  target.querySelector('button')!.click();
  flushSync();
  expect(target.textContent).toContain('Confirm speakers');
  [...target.querySelectorAll('button')].find((button) => button.textContent === 'Confirm speakers')!.click();
  await vi.waitFor(() => expect(getSession).toHaveBeenCalledWith('session-1'));
  await vi.waitFor(() => expect(target.textContent).toContain('Edit speakers'));
  expect(confirmSpeakerLabels).toHaveBeenCalledWith('session-1', { SPEAKER_0: 'Alice' });
  unmount(app);
});

it('refreshes the parent summary after renaming a confirmed speaker', async () => {
  const updated = session('confirmed', 'Therapist', 'Therapist spoke');
  getSession.mockResolvedValue(updated);
  updateSpeakerMap.mockResolvedValue('Therapist: hello');
  const props = $state({
    session: session('confirmed'),
    active: true,
    onUpdate: (next: Session) => { props.session = next; },
  });
  const target = document.createElement('div');
  const app = mount(SummaryPanel, { target, props });
  await vi.waitFor(() => expect(target.querySelector('button')?.textContent).toBe('Edit speakers'));
  target.querySelector('button')!.click();
  flushSync();
  await vi.waitFor(() => expect(target.querySelector('input')).not.toBeNull());
  const input = target.querySelector('input')!;
  input.value = 'Therapist';
  input.dispatchEvent(new Event('input', { bubbles: true }));
  input.dispatchEvent(new FocusEvent('blur'));
  await vi.waitFor(() => expect(target.textContent).toContain('Therapist spoke'));
  expect(updateSpeakerMap).toHaveBeenCalledWith('session-1', { SPEAKER_0: 'Therapist' });
  unmount(app);
});

it('does not publish a refresh after the card unmounts', async () => {
  let resolveSession!: (value: Session) => void;
  const onUpdate = vi.fn();
  getSession.mockReturnValue(new Promise((resolve) => { resolveSession = resolve; }));
  confirmSpeakerLabels.mockResolvedValue('Alice: hello');
  const target = document.createElement('div');
  const app = mount(SpeakersCard, { target, props: { session: session('pending'), onUpdate } });
  target.querySelector('button')!.click();
  flushSync();
  [...target.querySelectorAll('button')].find((button) => button.textContent === 'Confirm speakers')!.click();
  await vi.waitFor(() => expect(getSession).toHaveBeenCalledWith('session-1'));
  unmount(app);
  resolveSession(session('confirmed'));
  await Promise.resolve();
  expect(onUpdate).not.toHaveBeenCalled();
});

it('does not publish a refresh after the incoming session changes', async () => {
  let resolveSession!: (value: Session) => void;
  const onUpdate = vi.fn();
  getSession.mockReturnValue(new Promise((resolve) => { resolveSession = resolve; }));
  confirmSpeakerLabels.mockResolvedValue('Alice: hello');
  const props = $state({ session: session('pending'), onUpdate });
  const target = document.createElement('div');
  const app = mount(SpeakersCard, { target, props });
  target.querySelector('button')!.click();
  flushSync();
  [...target.querySelectorAll('button')].find((button) => button.textContent === 'Confirm speakers')!.click();
  await vi.waitFor(() => expect(getSession).toHaveBeenCalledWith('session-1'));
  props.session = { ...session('pending'), id: 'session-2' };
  flushSync();
  resolveSession(session('confirmed'));
  await Promise.resolve();
  expect(onUpdate).not.toHaveBeenCalled();
  unmount(app);
});
