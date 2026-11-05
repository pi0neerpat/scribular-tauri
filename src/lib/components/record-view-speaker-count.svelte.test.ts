// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { mount, unmount } from 'svelte';

const cancelJob = vi.fn();
const currentRecording = vi.fn();
const getRecordingSettings = vi.fn();
const getSession = vi.fn();
const onSessionProgress = vi.fn();
const onSessionSegments = vi.fn();
const startRecording = vi.fn();
const stopRecording = vi.fn();
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
