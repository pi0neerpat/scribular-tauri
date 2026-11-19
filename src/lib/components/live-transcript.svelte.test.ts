// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';
import type { SessionSegments } from '../types/SessionSegments';

const onSessionSegments = vi.hoisted(() => vi.fn());
vi.mock('../api', () => ({ onSessionSegments }));
const { default: LiveTranscript } = await import('./LiveTranscript.svelte');

async function render(waiting: boolean) {
  const target = document.createElement('div');
  const props = $state({ sessionId: 'session-a', labeled: false, waiting, activeElapsedMs: 0 });
  const app = mount(LiveTranscript, { target, props });
  await vi.waitFor(() => expect(onSessionSegments).toHaveBeenCalled());
  return { target, props, app };
}

describe('LiveTranscript waiting indication', () => {
  afterEach(() => onSessionSegments.mockReset());

  it('shows the message only while waiting without transcript text', async () => {
    let receive!: (segments: SessionSegments) => void;
    onSessionSegments.mockImplementation((handler: typeof receive) => {
      receive = handler;
      return Promise.resolve(() => {});
    });
    const { target, props, app } = await render(true);
    expect(target.textContent).toContain('Live transcript will start when the current summary finishes.');

    props.waiting = false;
    flushSync();
    expect(target.textContent).not.toContain('Live transcript will start');

    props.waiting = true;
    flushSync();
    receive({ sessionId: 'session-a', source: 'mic', segments: [{ startMs: 0, endMs: 1000, text: 'hello there' }] });
    flushSync();
    expect(target.textContent).toContain('hello there');
    expect(target.textContent).not.toContain('Live transcript will start');
    unmount(app);
  });

  it('shows no message when waiting was never reported', async () => {
    onSessionSegments.mockImplementation(() => Promise.resolve(() => {}));
    const { target, app } = await render(false);
    expect(target.textContent).not.toContain('Live transcript will start');
    unmount(app);
  });

  it('folds after twenty words from each source once shown 20 active seconds, and honors manual Show', async () => {
    let receive!: (segments: SessionSegments) => void;
    onSessionSegments.mockImplementation((handler: typeof receive) => { receive = handler; return Promise.resolve(() => {}); });
    const { target, props, app } = await render(false);
    const words = (n: number) => Array.from({ length: n }, (_, i) => `w${i}`).join(' ');
    props.labeled = true;
    props.activeElapsedMs = 5_000;
    receive({ sessionId: 'session-a', source: 'mic', segments: [{ startMs: 0, endMs: 1000, text: words(20) }] });
    flushSync();
    receive({ sessionId: 'session-a', source: 'system', segments: [{ startMs: 0, endMs: 1000, text: `[BLANK_AUDIO] ${words(20)}` }] });
    flushSync();
    // Both sources passed 20 words, but the transcript has shown for under 20s.
    props.activeElapsedMs = 24_999;
    flushSync();
    expect(target.querySelector('pre')).not.toBeNull();
    props.activeElapsedMs = 25_000;
    flushSync();
    expect(target.querySelector('pre')).toBeNull();
    [...target.querySelectorAll('button')].find((b) => b.textContent?.trim() === 'Show transcript')!.click();
    props.activeElapsedMs = 60_000;
    flushSync();
    expect(target.querySelector('pre')).not.toBeNull();
    props.sessionId = 'session-b';
    flushSync();
    receive({ sessionId: 'session-b', source: 'mic', segments: [{ startMs: 0, endMs: 1000, text: 'one' }] });
    flushSync();
    // Text first showed at 60s, so the time trigger still waits the 20s minimum.
    expect(target.querySelector('pre')).not.toBeNull();
    props.activeElapsedMs = 80_000;
    flushSync();
    expect(target.querySelector('pre')).toBeNull();
    unmount(app);
  });

  it('folds a one-source transcript at 60 active seconds below the word threshold', async () => {
    let receive!: (segments: SessionSegments) => void;
    onSessionSegments.mockImplementation((handler: typeof receive) => { receive = handler; return Promise.resolve(() => {}); });
    const { target, props, app } = await render(false);
    receive({ sessionId: 'session-a', source: 'mic', segments: [{ startMs: 0, endMs: 1000, text: 'just a few words' }] });
    flushSync();
    props.activeElapsedMs = 59_999;
    flushSync();
    expect(target.querySelector('pre')).not.toBeNull();
    props.activeElapsedMs = 60_000;
    flushSync();
    expect(target.querySelector('pre')).toBeNull();
    unmount(app);
  });

  it('does not count non-speech tags toward the fold threshold', async () => {
    let receive!: (segments: SessionSegments) => void;
    onSessionSegments.mockImplementation((handler: typeof receive) => { receive = handler; return Promise.resolve(() => {}); });
    const { target, app } = await render(false);
    receive({ sessionId: 'session-a', source: 'mic', segments: [{ startMs: 0, endMs: 1000, text: '[BLANK_AUDIO] [music] one two three' }] });
    flushSync();
    expect(target.querySelector('pre')).not.toBeNull();
    unmount(app);
  });
});
