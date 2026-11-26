// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { mount, unmount } from 'svelte';
import App from './App.svelte';
import { navigation } from './lib/navigation';
import type { CoachAvailability } from './lib/types/CoachAvailability';
import type { ModelProgress } from './lib/types/ModelProgress';
import type { Session } from './lib/types/Session';

const { invoke, listen, emit } = vi.hoisted(() => {
  const listeners = new Map<string, Set<(event: { payload: unknown }) => void>>();
  return {
    invoke: vi.fn<(command: string, args?: { enabled?: boolean }) => Promise<unknown>>(),
    listen: vi.fn(async (event: string, callback: (event: { payload: unknown }) => void) => {
      const callbacks = listeners.get(event) ?? new Set();
      listeners.set(event, callbacks);
      callbacks.add(callback);
      return () => callbacks.delete(callback);
    }),
    emit: (event: string, payload: unknown) => listeners.get(event)?.forEach((callback) => callback({ payload })),
  };
});

vi.mock('@tauri-apps/api/core', () => ({ invoke }));
vi.mock('@tauri-apps/api/event', () => ({ listen }));

const speech = (state: ModelProgress['state'], downloadedBytes = 0, error: string | null = null): ModelProgress =>
  ({ modelId: 'whisper-base', state, downloadedBytes, totalBytes: 100, error });
const seededSample = { id: 'sample', title: 'Example session', isSample: true, createdAt: Date.now(), status: 'done' };
const readyCoach: CoachAvailability = { reason: null, message: null, retry: null, sessionMessage: null };
const realNote: Session = {
  id: 'first-note', title: 'First note', createdAt: Date.now(), status: 'done', isSample: false,
  inputMode: 'mic', summary: 'Finished note', transcript: 'Transcript', error: null, promptId: null,
  modelId: null, plainTranscript: null, rawTranscript: null, speakerMap: null, diarizationState: 'none',
  durationMs: 1000, timings: null, hasAudio: false, audioExpiresAt: null, previousSummary: null,
  previousPromptId: null, previousModelId: null, canRedo: false, copiedAt: null,
};

let completed: boolean;
let consent: 'on' | 'off' | 'undecided';
let fail: string | undefined;
let snapshot: Promise<ModelProgress[]>;
let saveConsent: Promise<void>;
let firstNoteId: string | null;
let tipDismissed: boolean;
let app: ReturnType<typeof mount> | undefined;
let target: HTMLDivElement;

function launch() {
  app = mount(App, { target });
}

async function relaunch() {
  await unmount(app!);
  launch();
}

const dialog = () => target.querySelector('dialog[open]');
const checkbox = () => dialog()!.querySelector<HTMLInputElement>('input[type="checkbox"]')!;
const button = (label: string, root: ParentNode = target) =>
  [...root.querySelectorAll('button')].find((element) => element.textContent?.trim() === label)!;
const start = () => button('Start recording');
const next = () => button('Continue', dialog()!);
const retry = () => button('Retry', dialog()!);
const calls = (command: string) => invoke.mock.calls.filter(([name]) => name === command);
const reopenFromHelp = () => emit('shell://menu', 'shell-welcome');

beforeEach(() => {
  vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue(null);
  vi.stubGlobal('matchMedia', () => ({ matches: true }));
  HTMLDialogElement.prototype.showModal ??= function () { this.open = true; };
  HTMLDialogElement.prototype.close ??= function () { this.open = false; };
  completed = false;
  consent = 'undecided';
  fail = undefined;
  snapshot = Promise.resolve([speech('downloading', 40)]);
  saveConsent = Promise.resolve();
  firstNoteId = null;
  tipDismissed = false;
  navigation.set({ view: 'record', navigationLocked: false });
  target = document.body.appendChild(document.createElement('div'));
  invoke.mockImplementation(async (command, args) => {
    if (command === fail) throw new Error('IPC failed');
    switch (command) {
      case 'get_onboarding_state': return { completed, appAudioExplainerShown: false, firstNoteTipDismissed: tipDismissed };
      case 'complete_onboarding': completed = true; return;
      case 'get_telemetry_setting': return consent;
      case 'set_telemetry_setting':
        await saveConsent;
        consent = args?.enabled ? 'on' : 'off';
        return;
      case 'models_status': return snapshot;
      case 'app_version': return { name: 'Scribular', version: '0.1.0' };
      case 'get_app_settings': return { showBackgroundAnimation: false };
      case 'get_update_state': return { state: 'idle', installable: false };
      case 'get_recording_source': return { location: 'inRoom', targetApp: null };
      case 'list_sessions': return firstNoteId ? [seededSample, realNote] : [seededSample];
      case 'get_session':
        return (args as { id?: string })?.id === 'sample'
          ? { ...realNote, ...seededSample, summary: 'Sample note' }
          : realNote;
      case 'get_first_note_tip_session': return tipDismissed ? null : firstNoteId;
      case 'dismiss_first_note_tip': tipDismissed = true; return;
      case 'get_coach_availability': return readyCoach;
      case 'coach_open': return { id: 'draft', name: '', isAdjustment: false };
      case 'list_summary_models': return [];
      case 'locked_state':
      case 'current_recording':
      case 'get_recording_state': return null;
    }
  });
});

afterEach(async () => {
  if (app) await unmount(app);
  app = undefined;
  target.remove();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  vi.clearAllMocks();
});

describe('AS6 first-note tip', () => {
  const tip = () => [...target.querySelectorAll('aside')].find((el) => el.textContent?.includes('Want notes that look like yours?'));

  async function openSession(id: string, summary: string) {
    const row = () => target.querySelector<HTMLButtonElement>(`[data-session-id="${id}"]`);
    await vi.waitFor(() => expect(row()?.disabled).toBe(false));
    row()!.click();
    // The note is an editable textarea (R12); a lapsed licence shows it read-only in a <pre>.
    await vi.waitFor(() => expect([...target.querySelectorAll<HTMLTextAreaElement | HTMLPreElement>('[aria-label="Note"]')]
      .some((el) => ('value' in el ? el.value : el.textContent) === summary)).toBe(true));
  }

  it.each(['dismiss', 'open coach'])('persists after %s and relaunch', async (action) => {
    completed = true;
    snapshot = Promise.resolve([speech('ready')]);
    launch();
    await openSession('sample', 'Sample note');
    expect(tip()).toBeUndefined();

    firstNoteId = realNote.id;
    emit('session://progress', { sessionId: realNote.id, state: 'done' });
    await openSession(realNote.id, realNote.summary);
    await vi.waitFor(() => expect(tip()).toBeDefined());
    expect(calls('dismiss_first_note_tip')).toHaveLength(0);

    emit('coach-availability', { ...readyCoach, reason: 'recording', message: 'Finish recording first.' });
    const coachAction = () => button('New Style from Example Note…', tip()!);
    await vi.waitFor(() => expect(coachAction().getAttribute('aria-disabled')).toBe('true'));
    expect(tip()?.textContent).toContain('Finish recording first.');
    coachAction().click();
    expect(calls('coach_open')).toHaveLength(0);
    expect(button('Dismiss tip', tip()!).disabled).toBe(false);

    if (action === 'dismiss') {
      fail = 'dismiss_first_note_tip';
      button('Dismiss tip', tip()!).click();
      await vi.waitFor(() => expect(tip()?.textContent).toContain('Couldn’t save the dismissal.'));
      expect(tipDismissed).toBe(false);
      fail = undefined;
      button('Dismiss tip', tip()!).click();
    } else {
      emit('coach-availability', readyCoach);
      await vi.waitFor(() => expect(coachAction().getAttribute('aria-disabled')).toBe('false'));
      fail = 'coach_open';
      coachAction().click();
      await vi.waitFor(() => expect(dialog()?.textContent).toContain('Close it and try again.'));
      expect(calls('dismiss_first_note_tip')).toHaveLength(0);
      dialog()!.dispatchEvent(new Event('cancel', { cancelable: true }));
      await vi.waitFor(() => expect(button('Stop', dialog()!)).toBeDefined());
      button('Stop', dialog()!).click();
      await vi.waitFor(() => expect(dialog()).toBeNull());
      fail = undefined;
      coachAction().click();
      await vi.waitFor(() => expect(dialog()?.querySelector('[aria-current="step"]')?.textContent).toBe('Your example note'));
    }
    await vi.waitFor(() => expect(tip()).toBeUndefined());
    expect(tipDismissed).toBe(true);
    await relaunch();
    await openSession(realNote.id, realNote.summary);
    expect(tip()).toBeUndefined();
  });
});

describe('AS1 welcome on first launch', () => {
  it('opens one sheet, saves off before making it seen and skips it on reload during download', async () => {
    let finishSave!: () => void;
    saveConsent = new Promise((resolve) => { finishSave = resolve; });
    launch();
    await vi.waitFor(() => expect(dialog()).not.toBeNull());
    expect(target.querySelectorAll('dialog[open]')).toHaveLength(1);
    expect(dialog()?.textContent).toContain('Welcome to Scribular');
    expect(dialog()?.querySelectorAll('li')).toHaveLength(3);
    expect(next().disabled).toBe(true);
    expect(checkbox().checked).toBe(false);
    expect(calls('complete_onboarding')).toHaveLength(0);
    await vi.waitFor(() => expect(calls('download_models')).toHaveLength(1));
    expect(calls('open_privacy_settings')).toHaveLength(0);
    expect(target.textContent).toContain('Example session');
    finishSave();
    await vi.waitFor(() => expect(next().disabled).toBe(false));
    expect(consent).toBe('off');
    expect(calls('set_telemetry_setting')).toEqual([['set_telemetry_setting', { enabled: false }]]);
    expect(completed).toBe(true);

    await relaunch();
    await vi.waitFor(() => expect(target.textContent).toContain('Downloading the speech model (40%)'));
    expect(dialog()).toBeNull();
    expect(calls('set_telemetry_setting')).toHaveLength(1);
    expect(start().disabled).toBe(true);
  });

  it.each(['set_telemetry_setting', 'complete_onboarding'])('reopens with retry when %s fails', async (command) => {
    fail = command;
    launch();
    await vi.waitFor(() => expect(retry().disabled).toBe(false));
    expect(next().disabled).toBe(true);
    expect(consent).not.toBe('on');
    expect(completed).toBe(false);
    if (command === 'set_telemetry_setting') expect(calls('complete_onboarding')).toHaveLength(0);

    await relaunch();
    await vi.waitFor(() => expect(retry().disabled).toBe(false));
    expect(checkbox().checked).toBe(false);
    fail = undefined;
    retry().click();
    await vi.waitFor(() => expect(next().disabled).toBe(false));
    next().click();
    await vi.waitFor(() => expect(dialog()).toBeNull());
    expect(completed).toBe(true);
    expect(consent).toBe('off');
  });

  it('keeps the welcome in front of a tray start and lets quit confirmation take focus', async () => {
    snapshot = Promise.resolve([speech('ready', 100)]);
    launch();
    await vi.waitFor(() => expect(next().disabled || start().disabled).toBe(false));
    reopenFromHelp();
    emit('tray-start-recording', 7);
    await vi.waitFor(() => expect(calls('set_main_window_visible')).toHaveLength(2));
    expect(calls('get_recording_settings')).toHaveLength(0);
    expect(target.querySelectorAll('dialog[open]')).toHaveLength(1);

    emit('quit-requested', false);
    await vi.waitFor(() => expect(target.querySelector('[role="alertdialog"]')).not.toBeNull());
    expect(dialog()).toBeNull();
    button('Cancel').click();
    await vi.waitFor(() => expect(dialog()).not.toBeNull());
    expect(calls('complete_onboarding')).toHaveLength(1);
  });

  it('cancels a tray start while the startup gate is loading', async () => {
    const defaultInvoke = invoke.getMockImplementation()!;
    invoke.mockImplementation((command, args) =>
      command === 'locked_state' ? new Promise(() => {}) : defaultInvoke(command, args));
    launch();
    await vi.waitFor(() => expect(listen).toHaveBeenCalledWith('tray-start-recording', expect.any(Function)));
    emit('tray-start-recording', 7);
    expect(calls('cancel_tray_start')).toEqual([['cancel_tray_start', { request: 7 }]]);
    expect(calls('get_recording_settings')).toHaveLength(0);
  });
});

describe('AS2 Help and usage-data consent', () => {
  it.each([true, false])('recovers a failed startup read without guessing whether welcome was seen (completed=%s)', async (seen) => {
    completed = seen;
    fail = 'get_onboarding_state';
    launch();
    await vi.waitFor(() => expect(button('Retry welcome settings')).toBeDefined());
    expect(dialog()).toBeNull();
    expect(target.textContent).toContain('Could not read your welcome settings.');
    fail = undefined;
    button('Retry welcome settings').click();
    if (seen) {
      await vi.waitFor(() => expect(target.textContent).not.toContain('Could not read your welcome settings.'));
      expect(dialog()).toBeNull();
      expect(calls('set_telemetry_setting')).toHaveLength(0);
    } else {
      await vi.waitFor(() => expect(dialog()).not.toBeNull());
      await vi.waitFor(() => expect(next().disabled).toBe(false));
      expect(consent).toBe('off');
    }
  });

  it('reads the saved opt-in when Help reopens welcome and makes no writes', async () => {
    completed = true;
    consent = 'on';
    launch();
    await vi.waitFor(() => expect(calls('get_onboarding_state')).toHaveLength(2));
    expect(dialog()).toBeNull();
    reopenFromHelp();
    await vi.waitFor(() => expect(checkbox().checked && !next().disabled).toBe(true));
    next().click();
    await vi.waitFor(() => expect(dialog()).toBeNull());
    expect(calls('set_telemetry_setting')).toHaveLength(0);
    expect(calls('complete_onboarding')).toHaveLength(0);
  });

  it('surfaces read and save failures and only checks opt-in after an explicit save succeeds', async () => {
    completed = true;
    consent = 'off';
    launch();
    await vi.waitFor(() => expect(calls('get_onboarding_state')).toHaveLength(2));
    fail = 'get_telemetry_setting';
    reopenFromHelp();
    await vi.waitFor(() => expect(dialog()?.querySelector('[role=alert]')?.textContent).toContain('Could not read'));
    expect(next().disabled).toBe(true);
    fail = undefined;
    retry().click();
    await vi.waitFor(() => expect(checkbox().disabled).toBe(false));
    expect(consent).toBe('off');
    expect(calls('set_telemetry_setting')).toHaveLength(0);

    fail = 'set_telemetry_setting';
    checkbox().click();
    await vi.waitFor(() => expect(dialog()?.querySelector('[role=alert]')?.textContent).toContain('Could not save'));
    expect(checkbox().checked).toBe(false);
    expect(consent).toBe('off');
    expect(next().disabled).toBe(true);
    fail = undefined;
    retry().click();
    await vi.waitFor(() => expect(checkbox().checked && !next().disabled).toBe(true));
    expect(consent).toBe('on');
    expect(calls('complete_onboarding')).toHaveLength(0);
  });
});

describe('AS3 Record model download', () => {
  it('keeps speech progress ahead of a stale snapshot and lets a summary download run quietly', async () => {
    completed = true;
    let reply!: (status: ModelProgress[]) => void;
    snapshot = new Promise((resolve) => { reply = resolve; });
    launch();
    await vi.waitFor(() => expect(calls('models_status')).toHaveLength(1));
    const registration = listen.mock.calls.findIndex(([event]) => event === 'models://progress');
    const read = invoke.mock.calls.findIndex(([command]) => command === 'models_status');
    expect(listen.mock.invocationCallOrder[registration]).toBeLessThan(invoke.mock.invocationCallOrder[read]);
    const summary = { ...speech('downloading', 12), modelId: 'summary' };
    emit('models://progress', speech('downloading', 65));
    reply([speech('missing'), summary]);
    await vi.waitFor(() => expect(target.textContent).toContain('Downloading the speech model (65%)'));
    expect(target.querySelector<HTMLElement>('.record-ring')?.style.getPropertyValue('--speech-progress')).toBe('65%');
    expect(start().disabled).toBe(true);
    expect(button('New Recording').disabled).toBe(true);
    expect(target.querySelector<HTMLButtonElement>('button.session')?.disabled).toBe(true);
    expect(target.textContent).not.toMatch(/Getting Scribular ready|Waiting for the summary model|Summary model download|Help improve Scribular|Show tour/);

    emit('models://progress', speech('ready', 100));
    await vi.waitFor(() => expect(start().disabled).toBe(false));
    expect(button('New Recording').disabled).toBe(true);
    expect(start().hasAttribute('aria-describedby')).toBe(false);
    expect(dialog()).toBeNull();
  });

  it('retries a failed speech download beside Record and refreshes progress', async () => {
    completed = true;
    launch();
    await vi.waitFor(() => expect(calls('download_models')).toHaveLength(1));
    emit('models://progress', speech('failed', 0, 'connection lost'));
    await vi.waitFor(() => expect(target.textContent).toContain('Speech model download failed: connection lost'));
    expect(start().disabled).toBe(true);
    button('Retry speech model download').click();
    await vi.waitFor(() => expect(calls('download_models')).toHaveLength(2));
    expect(button('Retry speech model download').disabled).toBe(true);
    button('Retry speech model download').click();
    expect(calls('download_models')).toHaveLength(2);
    emit('models://progress', speech('downloading', 50));
    await vi.waitFor(() => expect(target.textContent).toContain('Downloading the speech model (50%)'));
    expect(button('Retry speech model download')).toBeUndefined();
    emit('models://progress', speech('failed', 50, 'connection lost again'));
    await vi.waitFor(() => expect(button('Retry speech model download').disabled).toBe(false));
  });

  it('retries when the initial status IPC fails', async () => {
    completed = true;
    fail = 'models_status';
    launch();
    await vi.waitFor(() => expect(target.textContent).toContain('Could not start or check the speech model download'));
    fail = undefined;
    button('Retry speech model download').click();
    await vi.waitFor(() => expect(target.textContent).toContain('Downloading the speech model (40%)'));
    expect(calls('download_models')).toHaveLength(1);
  });
});
