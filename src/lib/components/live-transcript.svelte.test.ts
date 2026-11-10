// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';
import type { SessionSegments } from '../types/SessionSegments';

const onSessionSegments = vi.hoisted(() => vi.fn());
vi.mock('../api', () => ({ onSessionSegments }));
const { default: LiveTranscript } = await import('./LiveTranscript.svelte');

async function render(waiting: boolean) {
  const target = document.createElement('div');
  const props = $state({ sessionId: 'session-a', labeled: false, waiting });
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
});
