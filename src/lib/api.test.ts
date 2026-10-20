import { beforeEach, describe, expect, it, vi } from 'vitest';

const invoke = vi.fn();
const listen = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({ invoke }));
vi.mock('@tauri-apps/api/event', () => ({ listen }));

const { appVersion, deniedPrivacy, onHeartbeat, startRecording } = await import('./api');

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
});
