// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { mount, unmount } from 'svelte';
import type { SessionProgress } from '../types/SessionProgress';
import type { Session } from '../types/Session';

const cancelJob = vi.fn();
const currentRecording = vi.fn();
const getRecordingSettings = vi.fn();
const getRecordingSource = vi.fn();
const getOnboardingState = vi.fn();
const markAppAudioExplainerShown = vi.fn();
const systemAudioPermissionStatus = vi.fn();
const openPrivacySettings = vi.fn();
const setRecordingSource = vi.fn();
const recordingState = vi.fn();
const onRecordingState = vi.fn();
const onSessionLevels = vi.fn();
const listAudioApps = vi.fn();
const deniedPrivacy = vi.fn(() => null as 'microphone' | 'systemAudio' | null);
const getSession = vi.fn();
const onSessionProgress = vi.fn();
const onSessionTranscriptWaiting = vi.fn(async (_handler: (state: { sessionId: string; waiting: boolean }) => void) => () => {});
const onSessionSegments = vi.fn();
const startRecording = vi.fn();
const stopRecording = vi.fn();
const getSummaryAvailability = vi.fn().mockResolvedValue('ready');
const onSummaryAvailability = vi.fn(async () => () => {});
let emitProgress: ((progress: SessionProgress) => void) | undefined;
let emitRecordingState: ((state: { sessionId: string; status: 'running'; activeElapsedMs: number; targetApp: string; appAudioStatus: 'permissionRequired' | 'capturing' | 'unavailable' }) => void) | undefined;
let emitTranscriptWaiting: ((state: { sessionId: string; waiting: boolean }) => void) | undefined;
vi.mock('../api', () => ({
  accessDenied: () => null, cancelJob, currentRecording, deniedPrivacy, getRecordingSettings, getSession,
  getRecordingSource, setRecordingSource, getOnboardingState, markAppAudioExplainerShown,
  systemAudioPermissionStatus, recordingState, onRecordingState, onSessionLevels,
  listAudioApps, openPrivacySettings, pauseRecording: vi.fn(), resumeRecording: vi.fn(),
  onSessionProgress, onSessionTranscriptWaiting, onSessionSegments, startRecording, stopRecording,
  getPromptList: vi.fn(() => new Promise(() => {})), listSummaryModels: vi.fn(async () => []), setActivePrompt: vi.fn(),
  getSummaryAvailability, onSummaryAvailability,
  subscribeSummaryAvailability: async (callback: (state: string) => void) => { callback(await getSummaryAvailability()); return () => {}; },
}));
const { default: RecordView } = await import('./RecordView.svelte');

const settings = (multiSpeaker: boolean) => ({
  inputMode: 'mic', targetApp: null, multiSpeaker, promptId: 'default', keepAudio: false,
});
const recording = { id: 'recording-1', inputMode: 'mic', status: 'recording', createdAt: 0 };

beforeEach(() => {
  HTMLDialogElement.prototype.showModal ??= function () { this.open = true; };
  HTMLDialogElement.prototype.close ??= function () { this.open = false; };
  sessionStorage.clear();
  currentRecording.mockResolvedValue(null);
  getRecordingSettings.mockResolvedValue(settings(true));
  getRecordingSource.mockResolvedValue({ location: 'inRoom', targetApp: null });
  getOnboardingState.mockResolvedValue({ completed: true, appAudioExplainerShown: false, firstNoteTipDismissed: false });
  markAppAudioExplainerShown.mockResolvedValue(undefined);
  systemAudioPermissionStatus.mockResolvedValue('unknown');
  openPrivacySettings.mockResolvedValue(undefined);
  recordingState.mockResolvedValue(null);
  onRecordingState.mockResolvedValue(() => {});
  emitRecordingState = undefined;
  onSessionLevels.mockResolvedValue(() => {});
  listAudioApps.mockResolvedValue([{ name: 'Playing Other App', icon: null }]);
  setRecordingSource.mockResolvedValue(undefined);
  getSession.mockResolvedValue(null);
  onSessionProgress.mockResolvedValue(() => {});
  emitProgress = undefined;
  emitTranscriptWaiting = undefined;
  onSessionSegments.mockResolvedValue(() => {});
  onSessionTranscriptWaiting.mockImplementation(async (handler) => {
    emitTranscriptWaiting = handler;
    return () => {};
  });
  startRecording.mockResolvedValue(recording);
  stopRecording.mockResolvedValue(undefined);
  deniedPrivacy.mockReturnValue(null);
});
afterEach(() => vi.clearAllMocks());

async function renderRecordView() {
  const target = document.createElement('div');
  document.body.append(target);
  const app = mount(RecordView, { target, props: { speech: { modelId: 'whisper-base', state: 'ready', downloadedBytes: 100, totalBytes: 100, error: null } } });
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
  it('shows the first-call sheet once, saves it, and returns to mic without capture', async () => {
    const { target, app } = await renderRecordView();
    await vi.waitFor(() => expect((target.querySelector('input[type="radio"]') as HTMLInputElement).disabled).toBe(false));
    expect(target.querySelector('input[value="system"]')).toBeNull();
    const call = target.querySelectorAll<HTMLInputElement>('input[type="radio"]')[1];
    call.click();
    await vi.waitFor(() => expect(target.querySelector('[aria-label="App audio access"]')).not.toBeNull());
    expect(markAppAudioExplainerShown).toHaveBeenCalledOnce();
    expect(openPrivacySettings).not.toHaveBeenCalled();
    expect(target.textContent).toContain('Permission unconfirmed');
    await clickButton(target, 'Open System Settings');
    expect(openPrivacySettings).toHaveBeenCalledExactlyOnceWith('systemAudio');
    await clickButton(target, 'Not Now');
    await vi.waitFor(() => expect(target.querySelector('[aria-label="App audio access"]')).toBeNull());
    expect(setRecordingSource).toHaveBeenCalledWith({ location: 'inRoom', targetApp: null });
    expect(target.querySelectorAll<HTMLInputElement>('input[type="radio"]')[0].checked).toBe(true);
    expect(call.checked).toBe(false);
    call.click();
    await vi.waitFor(() => expect(target.querySelector('[aria-label="Choose a call app"]')).not.toBeNull());
    expect(target.querySelector('[aria-label="App audio access"]')).toBeNull();
    expect(call.checked).toBe(true);
    expect([...target.querySelectorAll('button')].find((button) => button.textContent === 'Start recording')?.disabled).toBe(true);
    expect(startRecording).not.toHaveBeenCalled();
    unmount(app);
    target.remove();
  });
  it('retries a failed flag write and skips the explainer on relaunch', async () => {
    markAppAudioExplainerShown.mockRejectedValueOnce(new Error('store unavailable'));
    const first = await renderRecordView();
    await vi.waitFor(() => expect((first.target.querySelector('input[type="radio"]') as HTMLInputElement).disabled).toBe(false));
    first.target.querySelectorAll<HTMLInputElement>('input[type="radio"]')[1].click();
    await vi.waitFor(() => expect(first.target.textContent).toContain('store unavailable'));
    expect(openPrivacySettings).not.toHaveBeenCalled();
    await clickButton(first.target, 'Retry');
    await vi.waitFor(() => expect(markAppAudioExplainerShown).toHaveBeenCalledTimes(2));
    expect(openPrivacySettings).not.toHaveBeenCalled();
    unmount(first.app);
    first.target.remove();

    getOnboardingState.mockResolvedValue({ completed: true, appAudioExplainerShown: true, firstNoteTipDismissed: false });
    const second = await renderRecordView();
    await vi.waitFor(() => expect((second.target.querySelector('input[type="radio"]') as HTMLInputElement).disabled).toBe(false));
    second.target.querySelectorAll<HTMLInputElement>('input[type="radio"]')[1].click();
    await vi.waitFor(() => expect(second.target.querySelector('[aria-label="Choose a call app"]')).not.toBeNull());
    expect(second.target.querySelector('[aria-label="App audio access"]')).toBeNull();
    unmount(second.app);
    second.target.remove();
  });
  it('reports a macOS grant without starting capture', async () => {
    systemAudioPermissionStatus.mockResolvedValue('granted');
    const { target, app } = await renderRecordView();
    await vi.waitFor(() => expect((target.querySelector('input[type="radio"]') as HTMLInputElement).disabled).toBe(false));
    target.querySelectorAll<HTMLInputElement>('input[type="radio"]')[1].click();
    await vi.waitFor(() => expect(target.textContent).toContain('App audio access: Granted'));
    expect(startRecording).not.toHaveBeenCalled();
    unmount(app);
    target.remove();
  });
  it('clears a transient permission-status read error on retry', async () => {
    systemAudioPermissionStatus.mockRejectedValueOnce(new Error('status unavailable'));
    const { target, app } = await renderRecordView();
    await vi.waitFor(() => expect((target.querySelector('input[type="radio"]') as HTMLInputElement).disabled).toBe(false));
    target.querySelectorAll<HTMLInputElement>('input[type="radio"]')[1].click();
    await vi.waitFor(() => expect(target.textContent).toContain('status unavailable'));
    await clickButton(target, 'Retry');
    await vi.waitFor(() => expect(target.textContent).not.toContain('status unavailable'));
    unmount(app);
    target.remove();
  });
  it('shows an unconfirmed app-audio permission and clears it when capture attaches', async () => {
    getRecordingSource.mockResolvedValue({ location: 'onCall', targetApp: 'Zoom' });
    onRecordingState.mockImplementation(async (handler) => { emitRecordingState = handler; return () => {}; });
    const { target, app } = await renderRecordView();
    await vi.waitFor(() => expect(target.textContent).toContain('App: Zoom'));
    await clickButton(target, 'Start recording');
    await vi.waitFor(() => expect(target.textContent).toContain('Stop'));
    expect(target.textContent).not.toContain('Retry app audio');
    emitRecordingState!({ sessionId: recording.id, status: 'running', activeElapsedMs: 0, targetApp: 'Zoom', appAudioStatus: 'permissionRequired' });
    await vi.waitFor(() => expect(target.textContent).toContain('Retry app audio'));
    expect(target.textContent).toContain('App audio permission is required or unconfirmed.');
    emitRecordingState!({ sessionId: recording.id, status: 'running', activeElapsedMs: 0, targetApp: 'Zoom', appAudioStatus: 'capturing' });
    await vi.waitFor(() => expect(target.textContent).not.toContain('Retry app audio'));
    unmount(app);
    target.remove();
  });
  it('closes the first-call sheet when a tray start succeeds before progress arrives', async () => {
    getRecordingSource.mockResolvedValue({ location: 'onCall', targetApp: 'Zoom' });
    const { target, app } = await renderRecordView();
    await vi.waitFor(() => expect(target.textContent).toContain('App: Zoom'));
    target.querySelectorAll<HTMLInputElement>('input[type="radio"]')[0].click();
    await vi.waitFor(() => expect(target.querySelectorAll<HTMLInputElement>('input[type="radio"]')[0].checked).toBe(true));
    target.querySelectorAll<HTMLInputElement>('input[type="radio"]')[1].click();
    await vi.waitFor(() => expect(target.querySelector('[aria-label="App audio access"]')).not.toBeNull());
    window.dispatchEvent(new Event('tray-start-recording'));
    await vi.waitFor(() => expect(startRecording).toHaveBeenCalled());
    await vi.waitFor(() => expect(target.querySelector('[aria-label="App audio access"]')).toBeNull());
    unmount(app);
    target.remove();
  });
  it('keeps the app-audio-unavailable notice visible beside a recording error', async () => {
    getRecordingSource.mockResolvedValue({ location: 'onCall', targetApp: 'Zoom' });
    onRecordingState.mockImplementation(async (handler) => { emitRecordingState = handler; return () => {}; });
    stopRecording.mockRejectedValueOnce(new Error('stop failed'));
    const { target, app } = await renderRecordView();
    await vi.waitFor(() => expect(target.textContent).toContain('App: Zoom'));
    await clickButton(target, 'Start recording');
    await vi.waitFor(() => expect(target.textContent).toContain('Stop'));
    await clickButton(target, 'Stop');
    await clickButton(target, '2 people');
    await vi.waitFor(() => expect(target.textContent).toContain('stop failed'));
    emitRecordingState!({ sessionId: recording.id, status: 'running', activeElapsedMs: 0, targetApp: 'Zoom', appAudioStatus: 'unavailable' });
    await vi.waitFor(() => expect(target.textContent).toContain('App audio is unavailable.'));
    expect(target.textContent).toContain('stop failed');
    unmount(app);
    target.remove();
  });
  it('shows only the permission notice when microphone access is denied', async () => {
    startRecording.mockRejectedValueOnce(new Error('microphone permission denied'));
    deniedPrivacy.mockReturnValueOnce('microphone');
    const { target, app } = await renderRecordView();
    await clickButton(target, 'Start recording');
    await vi.waitFor(() => expect(target.textContent).toContain('Microphone access is off.'));
    expect(target.querySelectorAll('[role="alert"]')).toHaveLength(1);
    expect(target.textContent).not.toContain('microphone permission denied');
    unmount(app);
    target.remove();
  });
  it('chooses and persists a call app from the record screen', async () => {
    const { target, app } = await renderRecordView();
    await vi.waitFor(() => expect((target.querySelector('input[type="radio"]') as HTMLInputElement).disabled).toBe(false));
    const call = target.querySelectorAll<HTMLInputElement>('input[type="radio"]')[1];
    call.click();
    await vi.waitFor(() => expect(target.querySelector('[aria-label="App audio access"]')).not.toBeNull());
    await clickButton(target, 'Choose an app to try recording');
    await vi.waitFor(() => expect(target.textContent).toContain('Playing Other App'));
    await clickButton(target, 'Playing Other App');
    expect(setRecordingSource).toHaveBeenCalledWith({ location: 'onCall', targetApp: 'Playing Other App' });
    await vi.waitFor(() => expect(target.textContent).toContain('App: Playing Other App'));
    await clickButton(target, 'Start recording');
    expect(startRecording).toHaveBeenCalledWith('micAndSystem', 'Playing Other App', expect.any(String));
    unmount(app);
    target.remove();
  });

  it('keeps the app picker inactive during a slow Start', async () => {
    getRecordingSource.mockResolvedValue({ location: 'onCall', targetApp: 'Zoom' });
    startRecording.mockReturnValue(new Promise(() => {}));
    const { target, app } = await renderRecordView();
    await vi.waitFor(() => expect(target.textContent).toContain('App: Zoom'));
    await clickButton(target, 'Start recording');
    await vi.waitFor(() => expect(startRecording).toHaveBeenCalled());
    const picker = [...target.querySelectorAll('button')].find((button) => button.textContent?.includes('App: Zoom'))!;
    expect(picker.disabled).toBe(true);
    picker.click();
    expect(target.querySelector('[aria-label="Choose a call app"]')).toBeNull();
    unmount(app);
    target.remove();
  });

  it('clears system audio denial after app discovery succeeds', async () => {
    const { target, app } = await renderRecordView();
    await vi.waitFor(() => expect((target.querySelector('input[type="radio"]') as HTMLInputElement).disabled).toBe(false));
    const call = target.querySelectorAll<HTMLInputElement>('input[type="radio"]')[1];
    call.click();
    await vi.waitFor(() => expect(target.querySelector('[aria-label="App audio access"]')).not.toBeNull());
    await clickButton(target, 'Choose an app to try recording');
    listAudioApps.mockRejectedValueOnce(new Error('system audio recording permission denied'));
    deniedPrivacy.mockReturnValueOnce('systemAudio');
    await clickButton(target, 'Refresh');
    await vi.waitFor(() => expect(target.textContent).toContain('Retry app audio'));
    await clickButton(target, 'Retry app audio');
    await vi.waitFor(() => expect(target.querySelector('[aria-label="App audio access"]')).not.toBeNull());
    expect(openPrivacySettings).not.toHaveBeenCalled();
    await clickButton(target, 'Open System Settings');
    expect(openPrivacySettings).toHaveBeenCalledWith('systemAudio');
    unmount(app);
    target.remove();
  });

  it('keeps a short active transcript expanded after a long paused recording', async () => {
    let emitSegments!: (event: { sessionId: string; source: 'mic'; segments: { startMs: number; endMs: number; text: string }[] }) => void;
    onSessionSegments.mockImplementation(async (handler) => { emitSegments = handler; return () => {}; });
    recordingState.mockResolvedValue({ sessionId: recording.id, status: 'paused', activeElapsedMs: 10_000 });
    const clock = vi.spyOn(Date, 'now').mockReturnValue(1_000);
    const { target, app } = await renderRecordView();
    await clickButton(target, 'Start recording');
    await vi.waitFor(() => expect(target.textContent).toContain('Stop'));
    emitSegments({ sessionId: recording.id, source: 'mic', segments: [{ startMs: 0, endMs: 1000, text: 'hello there' }] });
    await vi.waitFor(() => expect(target.querySelector('pre')?.textContent).toContain('hello there'));
    clock.mockReturnValue(62_000);
    await new Promise((resolve) => setTimeout(resolve, 1100));
    await clickButton(target, 'Stop');
    await clickButton(target, '2 people');
    await vi.waitFor(() => expect(stopRecording).toHaveBeenCalled());
    expect(target.querySelector('pre')?.textContent).toContain('hello there');
    clock.mockRestore();
    unmount(app);
    target.remove();
  });
  it('allows starting a recording while a summary is running', async () => {
    sessionStorage.setItem('scribular.inFlightSession', 'summary-1');
    getSession.mockResolvedValueOnce({ id: 'summary-1', status: 'summarizing', inputMode: 'mic', createdAt: 0 });
    const { target, app } = await renderRecordView();
    await vi.waitFor(() => expect(target.textContent).toContain('Working…'));
    expect([...target.querySelectorAll('button')].map((button) => button.textContent?.trim())).toEqual(
      expect.arrayContaining(['Start recording', 'Cancel']),
    );
    await clickButton(target, 'Start recording');
    await vi.waitFor(() => expect(target.textContent).toContain('Stop'));
    expect(startRecording).toHaveBeenCalledTimes(1);
    unmount(app);
    target.remove();
  });

  it('shows an early transcript-waiting event after startup resolves and hides it when cleared', async () => {
    let resolveStart!: (value: typeof recording) => void;
    startRecording.mockReturnValueOnce(new Promise((resolve) => { resolveStart = resolve; }));
    const { target, app } = await renderRecordView();
    await clickButton(target, 'Start recording');
    await vi.waitFor(() => expect(startRecording).toHaveBeenCalled());

    emitTranscriptWaiting!({ sessionId: recording.id, waiting: true });
    resolveStart(recording);
    await vi.waitFor(() => expect(target.textContent).toContain('Live transcript will start when the current summary finishes.'));

    emitTranscriptWaiting!({ sessionId: recording.id, waiting: false });
    await vi.waitFor(() => expect(target.textContent).not.toContain('Live transcript will start when the current summary finishes.'));
    unmount(app);
    target.remove();
  });

  it('keeps Start available and offers Cancel while awaiting the summary model', async () => {
    sessionStorage.setItem('scribular.inFlightSession', 'waiting-1');
    getSession.mockResolvedValueOnce({ id: 'waiting-1', status: 'awaitingModel', inputMode: 'mic', createdAt: 0 });
    const { target, app } = await renderRecordView();
    await vi.waitFor(() => expect(target.textContent).toContain('Waiting for the summary model download'));
    expect([...target.querySelectorAll('button')].map((button) => button.textContent?.trim())).toEqual(
      expect.arrayContaining(['Start recording', 'Cancel']),
    );
    await clickButton(target, 'Cancel');
    expect(cancelJob).toHaveBeenCalledWith('waiting-1');
    unmount(app);
    target.remove();
  });

  it('clears the displayed job after its session is deleted', async () => {
    sessionStorage.setItem('scribular.inFlightSession', 'queued-1');
    getSession.mockResolvedValueOnce({ id: 'queued-1', status: 'awaitingModel', inputMode: 'mic', createdAt: 0 });
    const { target, app } = await renderRecordView();
    await vi.waitFor(() => expect(target.textContent).toContain('Waiting for the summary model download'));
    await app.refreshAfterDeletion();
    await vi.waitFor(() => expect(target.textContent).not.toContain('Waiting for the summary model download'));
    expect(sessionStorage.getItem('scribular.inFlightSession')).toBeNull();
    unmount(app);
    target.remove();
  });

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

  it('keeps the recording clock running when Stop fails', async () => {
    const clock = vi.spyOn(Date, 'now').mockReturnValue(1_000);
    stopRecording.mockRejectedValueOnce(new Error('stop failed'));
    const { target, app } = await renderRecordView();
    await clickButton(target, 'Start recording');
    await vi.waitFor(() => expect(target.textContent).toContain('Stop'));
    await clickButton(target, 'Stop');
    await clickButton(target, '2 people');
    await vi.waitFor(() => expect(target.textContent).toContain('stop failed'));
    clock.mockReturnValue(4_000);
    await vi.waitFor(() => expect(target.textContent).toMatch(/Recording…\s+0:03/), { timeout: 2_000 });
    clock.mockRestore();
    unmount(app);
    target.remove();
  });

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

  it('ignores an older fetch after a newer update for the same session', async () => {
    let resolveOld!: (session: Session) => void;
    let resolveNew!: (session: Session) => void;
    const result: Session = {
      id: recording.id, inputMode: 'mic', status: 'done', title: '', createdAt: 0,
      error: null, promptId: null, modelId: null,
      transcript: '', plainTranscript: null, rawTranscript: null, speakerMap: null,
      diarizationState: 'none', summary: '', durationMs: null, timings: null,
      hasAudio: false, isSample: false, audioExpiresAt: null,
      previousSummary: null, previousPromptId: null, previousModelId: null, canRedo: false, copiedAt: null,
    } as const;
    onSessionProgress.mockImplementation(async (callback) => {
      emitProgress = callback;
      return () => {};
    });
    getSession
      .mockReturnValueOnce(new Promise((resolve) => { resolveOld = resolve; }))
      .mockReturnValueOnce(new Promise((resolve) => { resolveNew = resolve; }));
    const { target, app } = await renderRecordView();
    await clickButton(target, 'Start recording');
    await vi.waitFor(() => expect(target.textContent).toContain('Stop'));
    emitProgress!({ sessionId: recording.id, state: 'done', stepIndex: 1, stepCount: 1, message: 'Done' });
    emitProgress!({ sessionId: recording.id, state: 'done', stepIndex: 2, stepCount: 2, message: 'Done' });
    await vi.waitFor(() => expect(getSession).toHaveBeenCalledTimes(2));

    resolveNew({ ...result, id: recording.id, title: 'new result', summary: 'new summary' });
    await vi.waitFor(() => expect(target.textContent).toContain('new summary'));
    resolveOld({ ...result, id: recording.id, title: 'old result', summary: 'old summary' });
    await Promise.resolve();
    expect(target.textContent).toContain('new summary');
    expect(target.textContent).not.toContain('old summary');
    unmount(app);
    target.remove();
  });

  it('uses the recording-start setting after the saved setting changes and closes on Escape', async () => {
    const { target, app } = await renderRecordView();
    await clickButton(target, 'Start recording');
    await vi.waitFor(() => expect(target.textContent).toContain('Stop'));

    getRecordingSettings.mockResolvedValue(settings(false));
    const stopButton = [...target.querySelectorAll('button')].find((button) => button.textContent === 'Stop');
    await clickButton(target, 'Stop');
    const dialog = target.querySelector('[role="dialog"]')!;
    await vi.waitFor(() => expect(dialog.contains(document.activeElement)).toBe(true));
    expect(dialog.getAttribute('aria-modal')).toBe('true');
    expect(target.querySelector('section')!.inert).toBe(true);
    const choices = dialog.querySelectorAll('button');
    choices[choices.length - 1].focus();
    dialog.dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', bubbles: true, cancelable: true }));
    expect(document.activeElement).toBe(choices[0]);
    choices[0].focus();
    dialog.dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', shiftKey: true, bubbles: true, cancelable: true }));
    expect(document.activeElement).toBe(choices[choices.length - 1]);
    dialog.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    await vi.waitFor(() => expect(target.querySelector('[role="dialog"]')).toBeNull());
    expect(target.querySelector('section')!.inert).toBe(false);
    expect(stopRecording).not.toHaveBeenCalled();
    expect(document.activeElement).toBe(stopButton);
    expect(getRecordingSettings).toHaveBeenCalledTimes(1);
    unmount(app);
    target.remove();
  });
});
