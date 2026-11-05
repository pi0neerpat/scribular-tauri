// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { mount, unmount } from 'svelte';
import type { SessionProgress } from '../types/SessionProgress';

const cancelJob = vi.fn();
const currentRecording = vi.fn();
const getRecordingSettings = vi.fn();
const getSession = vi.fn();
const onSessionProgress = vi.fn();
const onSessionSegments = vi.fn();
const startRecording = vi.fn();
const stopRecording = vi.fn();
let emitProgress: ((progress: SessionProgress) => void) | undefined;
vi.mock('../api', () => ({
  cancelJob, currentRecording, deniedPrivacy: () => null, getRecordingSettings, getSession,
  onSessionProgress, onSessionSegments, startRecording, stopRecording,
}));
const { default: RecordView } = await import('./RecordView.svelte');

const settings = (multiSpeaker: boolean) => ({
  inputMode: 'mic', targetApp: null, multiSpeaker, promptId: 'default', keepAudio: false,
});
const recording = { id: 'recording-1', inputMode: 'mic', status: 'recording', createdAt: 0 };

beforeEach(() => {
  sessionStorage.clear();
  currentRecording.mockResolvedValue(null);
  getRecordingSettings.mockResolvedValue(settings(true));
  getSession.mockResolvedValue(null);
  onSessionProgress.mockResolvedValue(() => {});
  emitProgress = undefined;
  onSessionSegments.mockResolvedValue(() => {});
  startRecording.mockResolvedValue(recording);
  stopRecording.mockResolvedValue(undefined);
});
afterEach(() => vi.clearAllMocks());

async function renderRecordView() {
  const target = document.createElement('div');
  document.body.append(target);
  const app = mount(RecordView, { target, props: { speechReady: true } });
  await vi.waitFor(() => expect(target.querySelector('button')).not.toBeNull());
  return { target, app };
}

async function clickButton(target: HTMLElement, name: string) {
  const button = [...target.querySelectorAll('button')].find((node) => node.textContent?.trim() === name);
  expect(button, `button ${name}`).toBeTruthy();
  button!.click();
  await Promise.resolve();
}

describe('recording speaker-count prompt', () => {
  it.each([["2 people", 2], ["3 people", 3], ["4+ (beta)", null]] as const)(
    'forwards %s to stopRecording', async (choice, count) => {
      const { target, app } = await renderRecordView();
      await clickButton(target, 'Start recording');
      await vi.waitFor(() => expect(target.textContent).toContain('Stop'));
      await clickButton(target, 'Stop');
      await clickButton(target, choice);
      await vi.waitFor(() => expect(stopRecording).toHaveBeenCalledWith(count));
      unmount(app);
      target.remove();
    },
  );

  it('stores the speaker marker before startup IPC and clears it if startup fails', async () => {
    let resolveStart!: (value: typeof recording) => void;
    startRecording.mockReturnValueOnce(new Promise((resolve) => { resolveStart = resolve; }));
    const { target, app } = await renderRecordView();
    await clickButton(target, 'Start recording');
    await vi.waitFor(() => expect(startRecording).toHaveBeenCalled());
    expect(sessionStorage.getItem('scribular.inFlightMultiSpeaker')).toBe('true');
    resolveStart(recording);
    await vi.waitFor(() => expect(sessionStorage.getItem('scribular.inFlightSession')).toBe(recording.id));
    unmount(app);
    target.remove();

    sessionStorage.clear();
    startRecording.mockRejectedValueOnce(new Error('startup failed'));
    const failed = await renderRecordView();
    await clickButton(failed.target, 'Start recording');
    await vi.waitFor(() => expect(failed.target.textContent).toContain('startup failed'));
    expect(sessionStorage.getItem('scribular.inFlightMultiSpeaker')).toBeNull();
    unmount(failed.app);
    failed.target.remove();

    sessionStorage.clear();
    let rejectOld!: (error: Error) => void;
    startRecording.mockReturnValueOnce(new Promise((_, reject) => { rejectOld = reject; }));
    const old = await renderRecordView();
    await clickButton(old.target, 'Start recording');
    await vi.waitFor(() => expect(startRecording).toHaveBeenCalledTimes(3));
    unmount(old.app);
    old.target.remove();

    let resolveNew!: (value: typeof recording) => void;
    startRecording.mockReturnValueOnce(new Promise((resolve) => { resolveNew = resolve; }));
    const fresh = await renderRecordView();
    await clickButton(fresh.target, 'Start recording');
    await vi.waitFor(() => expect(startRecording).toHaveBeenCalledTimes(4));
    const freshAttempt = sessionStorage.getItem('scribular.inFlightStartAttempt');
    rejectOld(new Error('old startup failed'));
    await vi.waitFor(() => expect(fresh.target.textContent).not.toContain('old startup failed'));
    expect(sessionStorage.getItem('scribular.inFlightMultiSpeaker')).toBe('true');
    expect(sessionStorage.getItem('scribular.inFlightStartAttempt')).toBe(freshAttempt);
    resolveNew(recording);
    await vi.waitFor(() => expect(sessionStorage.getItem('scribular.inFlightSession')).toBe(recording.id));
    unmount(fresh.app);
    fresh.target.remove();
  });

  it('keeps a new attempt marker when an older session finishes', async () => {
    onSessionProgress.mockImplementation(async (callback) => {
      emitProgress = callback;
      return () => {};
    });
    const { target, app } = await renderRecordView();
    await clickButton(target, 'Start recording');
    await vi.waitFor(() => expect(target.textContent).toContain('Stop'));
    emitProgress!({ sessionId: recording.id, state: 'done', stepIndex: 1, stepCount: 1, message: 'Done' });
    await vi.waitFor(() => expect(target.textContent).toContain('Start recording'));

    let rejectStart!: (error: Error) => void;
    startRecording.mockReturnValueOnce(new Promise((_, reject) => { rejectStart = reject; }));
    await clickButton(target, 'Start recording');
    await vi.waitFor(() => expect(startRecording).toHaveBeenCalledTimes(2));
    const attempt = sessionStorage.getItem('scribular.inFlightStartAttempt');
    emitProgress!({ sessionId: recording.id, state: 'done', stepIndex: 1, stepCount: 1, message: 'Done' });
    expect(sessionStorage.getItem('scribular.inFlightStartAttempt')).toBe(attempt);
    expect(sessionStorage.getItem('scribular.inFlightMultiSpeaker')).toBe('true');
    rejectStart(new Error('new startup failed'));
    await vi.waitFor(() => expect(target.textContent).toContain('new startup failed'));
    expect(sessionStorage.getItem('scribular.inFlightStartAttempt')).toBeNull();
    expect(sessionStorage.getItem('scribular.inFlightMultiSpeaker')).toBeNull();
    unmount(app);
    target.remove();
  });

  it('uses the recording-start setting after the saved setting changes and confines focus', async () => {
    const { target, app } = await renderRecordView();
    await clickButton(target, 'Start recording');
    await vi.waitFor(() => expect(target.textContent).toContain('Stop'));

    getRecordingSettings.mockResolvedValue(settings(false));
    const stopButton = [...target.querySelectorAll('button')].find((button) => button.textContent === 'Stop');
    await clickButton(target, 'Stop');
    const dialog = target.querySelector('[role="dialog"]')!;
    await vi.waitFor(() => expect(dialog.contains(document.activeElement)).toBe(true));
    const buttons = [...dialog.querySelectorAll('button')];
    buttons.at(-1)!.focus();
    buttons.at(-1)!.dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', bubbles: true }));
    expect(document.activeElement).toBe(buttons[0]);
    buttons[0].dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', shiftKey: true, bubbles: true }));
    expect(document.activeElement).toBe(buttons.at(-1));

    await clickButton(target, 'Keep recording');
    expect(document.activeElement).toBe(stopButton);
    expect(getRecordingSettings).toHaveBeenCalledTimes(1);
    unmount(app);
    target.remove();
  });
});
