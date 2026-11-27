import { afterEach, describe, expect, it, vi } from 'vitest';

const requestSessionAudio = vi.fn();
vi.mock('../api', () => ({ requestSessionAudio }));
const { audioUnavailable, currentLine, formatTime, openAudio, retentionLine } = await import('./session-audio');

describe('session audio (AS19)', () => {
  afterEach(() => requestSessionAudio.mockReset());

  it('wraps the bytes in a revocable blob URL', async () => {
    requestSessionAudio.mockResolvedValue(new Uint8Array(44));
    const view = await openAudio('s1');
    expect(requestSessionAudio).toHaveBeenCalledWith('s1');
    if (view.kind !== 'player') throw new Error(`got ${view.kind}`);
    expect(view.url).toMatch(/^blob:/);
    URL.revokeObjectURL(view.url);
  });

  it.each([
    ['notFound', { kind: 'hidden' }],
    ['expired', { kind: 'message', text: 'This recording has been deleted' }],
    ['tooLarge', { kind: 'message', text: 'Recording too long to play in the app' }],
    ['failed', { kind: 'message', text: "Couldn't load the recording" }],
    [new Error('boom'), { kind: 'message', text: "Couldn't load the recording" }],
  ])('maps %s without throwing', async (error, view) => {
    requestSessionAudio.mockRejectedValue(error);
    await expect(openAudio('s1')).resolves.toEqual(view);
  });

  it('shows the date, or today on the last day', () => {
    const now = new Date(2026, 10, 4, 9).getTime();
    expect(retentionLine(new Date(2026, 10, 11, 9).getTime(), now)).toBe('Recording deletes Nov 11');
    expect(retentionLine(new Date(2026, 10, 4, 23).getTime(), now)).toBe('Recording deletes today');
  });
});

describe('player helpers (R15)', () => {
  it('highlights the last started line and formats times', () => {
    const lines = [{ startMs: 0 }, { startMs: 4000 }, { startMs: 9000 }];
    expect(currentLine(lines, 3999)).toBe(0);
    expect(currentLine(lines, 4000)).toBe(1);
    expect(currentLine([{ startMs: 500 }], 0)).toBe(-1);
    expect(formatTime(65.9)).toBe('1:05');
    expect(formatTime(NaN)).toBe('0:00');
  });

  it('explains why the Audio tab is disabled', () => {
    expect(audioUnavailable({ hasAudio: true, audioExpiresAt: 2000 }, 1000)).toBeNull();
    expect(audioUnavailable({ hasAudio: true, audioExpiresAt: 2000 }, 2000)).toContain('deleted after seven days');
    expect(audioUnavailable({ hasAudio: false, audioExpiresAt: null })).toContain('No audio was kept');
  });
});
