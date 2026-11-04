import { afterEach, describe, expect, it, vi } from 'vitest';

const requestSessionAudio = vi.fn();
vi.mock('../api', () => ({ requestSessionAudio }));
const { openAudio, retentionLine } = await import('./session-audio');

describe('session audio (AS19)', () => {
  afterEach(() => requestSessionAudio.mockReset());

  it('wraps the bytes in a revocable blob URL', async () => {
    requestSessionAudio.mockResolvedValue(new ArrayBuffer(44));
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
