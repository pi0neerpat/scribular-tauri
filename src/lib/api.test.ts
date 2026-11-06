import { beforeEach, describe, expect, it, vi } from 'vitest';

const invoke = vi.fn();
const listen = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({ invoke }));
vi.mock('@tauri-apps/api/event', () => ({ listen }));

const {
  accessDenied,
  appVersion,
  checkForUpdates,
  getUpdateState,
  installUpdate,
  deniedPrivacy,
  onHeartbeat,
  onSessionSegments,
  onUpdateState,
  requestSessionAudio,
  startFresh,
  startRecording,
} = await import('./api');

describe('api', () => {
  beforeEach(() => {
    invoke.mockReset();
    listen.mockReset();
  });

  it('appVersion invokes app_version', async () => {
    invoke.mockResolvedValue({ name: 'scribular', version: '0.1.0' });
    await expect(appVersion()).resolves.toEqual({ name: 'scribular', version: '0.1.0' });
    expect(invoke).toHaveBeenCalledWith('app_version');
  });

  it('onHeartbeat unwraps the event payload', async () => {
    listen.mockResolvedValue(() => {});
    const seen: number[] = [];
    await onHeartbeat((beat) => seen.push(beat.seq));
    const [name, cb] = listen.mock.calls[0];
    expect(name).toBe('app://heartbeat');
    cb({ payload: { seq: 3 } });
    expect(seen).toEqual([3]);
  });

  it('wraps updater commands and event', async () => {
    invoke.mockResolvedValue({ state: 'idle', installable: false });
    await checkForUpdates();
    await expect(getUpdateState()).resolves.toEqual({ state: 'idle', installable: false });
    await installUpdate();
    expect(invoke.mock.calls.map(([name]) => name)).toEqual(['check_for_updates', 'get_update_state', 'install_update']);
    listen.mockResolvedValue(() => {});
    const handler = vi.fn();
    await onUpdateState(handler);
    expect(listen.mock.calls[0][0]).toBe('update://state');
    listen.mock.calls[0][1]({ payload: { state: 'idle', installable: false } });
    expect(handler).toHaveBeenCalledWith({ state: 'idle', installable: false });
  });

  it('onSessionSegments listens on session://segments', async () => {
    listen.mockResolvedValue(() => {});
    const seen: string[] = [];
    await onSessionSegments((s) => seen.push(s.source));
    const [name, cb] = listen.mock.calls[0];
    expect(name).toBe('session://segments');
    cb({ payload: { sessionId: 's1', source: 'mic', segments: [] } });
    expect(seen).toEqual(['mic']);
  });

  it('requestSessionAudio returns bytes from an ArrayBuffer or a number[] and passes typed errors through', async () => {
    invoke.mockResolvedValue(new Uint8Array([82, 73, 70, 70]).buffer);
    await expect(requestSessionAudio('s1')).resolves.toEqual(new Uint8Array([82, 73, 70, 70]));
    invoke.mockResolvedValue([82, 73, 70, 70]); // what the release webview actually delivered
    await expect(requestSessionAudio('s1')).resolves.toEqual(new Uint8Array([82, 73, 70, 70]));
    expect(invoke).toHaveBeenCalledWith('request_session_audio', { id: 's1' });
    invoke.mockRejectedValue('expired');
    await expect(requestSessionAudio('s1')).rejects.toBe('expired');
  });

  it('startFresh passes the typed confirmation', async () => {
    invoke.mockResolvedValue(undefined);
    await startFresh('start fresh');
    expect(invoke).toHaveBeenCalledWith('start_fresh', { confirmation: 'start fresh' });
  });

  it('startRecording sends camelCase arguments', async () => {
    invoke.mockResolvedValue({ id: 's1' });
    await startRecording('micAndSystem', 'Zoom', 'Session');
    expect(invoke).toHaveBeenCalledWith('start_recording', {
      mode: 'micAndSystem',
      appId: 'Zoom',
      title: 'Session',
    });
  });

  it('deniedPrivacy maps permission errors to their pane', () => {
    expect(deniedPrivacy('audio: microphone permission denied')).toBe('microphone');
    expect(deniedPrivacy('audio: system audio recording permission denied')).toBe('systemAudio');
    expect(deniedPrivacy('choose an app to record in Settings')).toBeNull();
  });

  it('accessDenied reads the licence refusal and nothing else', () => {
    const state = { kind: 'trialEnded' };
    expect(accessDenied(JSON.stringify({ kind: 'accessDenied', state }))).toEqual(state);
    expect(accessDenied('microphone permission denied')).toBeNull();
    expect(accessDenied(JSON.stringify({ kind: 'other', state }))).toBeNull();
    expect(accessDenied('null')).toBeNull();
  });
});
