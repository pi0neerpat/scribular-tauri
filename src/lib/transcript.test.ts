import { describe, expect, it } from 'vitest';
import { liveTranscript } from './transcript';

const seg = (startMs: number, text: string) => ({ startMs, endMs: startMs + 500, text });

describe('liveTranscript', () => {
  it('joins one source', () => {
    expect(liveTranscript({ mic: [seg(0, ' Hello'), seg(900, ' there ')], system: [] }, false)).toBe(
      'Hello there',
    );
  });

  it('drops tags from one source', () => {
    expect(liveTranscript({ mic: [seg(0, '[BLANK_AUDIO]'), seg(900, 'Hi [music] there')], system: [] }, false)).toBe(
      'Hi  there',
    );
  });

  it('merges two sources into speaker turns', () => {
    const text = liveTranscript(
      {
        mic: [seg(0, 'Hi.'), seg(1000, 'Same time.'), seg(1500, '[BLANK_AUDIO]'), seg(1600, 'Bye.')],
        system: [seg(1000, ' Hello [music] back.')],
      },
      true,
    );
    expect(text).toBe('You: Hi.\nClient: Hello  back.\nYou: Same time. Bye.');
  });
});
