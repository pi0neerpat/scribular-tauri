// @vitest-environment jsdom
// AS19: the player component revokes its blob URL on unmount, on session
// change, when hidden, and at the retention deadline.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';
import type { Session } from '../types/Session';

const requestSessionAudio = vi.fn();
vi.mock('../api', () => ({ requestSessionAudio }));
const { default: SessionAudio } = await import('./SessionAudio.svelte');

let urls = 0;
const revoke = vi.fn();

function session(id: string, audioExpiresAt: number | null = null): Session {
  return { id, hasAudio: true, audioExpiresAt } as Session;
}

async function render(props: { session: Session; active: boolean }) {
  const target = document.createElement('div');
  const app = mount(SessionAudio, { target, props });
  await vi.waitFor(() => expect(target.querySelector('audio')).not.toBeNull());
  return { target, app };
}

describe('SessionAudio component (AS19)', () => {
  beforeEach(() => {
    urls = 0;
    URL.createObjectURL = () => `blob:test/${++urls}`;
    URL.revokeObjectURL = revoke;
    requestSessionAudio.mockResolvedValue(new Uint8Array(44));
  });
  afterEach(() => {
    revoke.mockReset();
    requestSessionAudio.mockReset();
    vi.useRealTimers();
  });

  it('renders the player and revokes on unmount', async () => {
    const { target, app } = await render({ session: session('a'), active: true });
    expect(target.querySelector('audio')?.getAttribute('src')).toBe('blob:test/1');
    unmount(app);
    expect(revoke).toHaveBeenCalledWith('blob:test/1');
  });

  it('revokes the old URL when the session changes', async () => {
    const props = $state({ session: session('a'), active: true });
    const { target, app } = await render(props);
    props.session = session('b');
    flushSync();
    expect(revoke).toHaveBeenCalledWith('blob:test/1');
    await vi.waitFor(() =>
      expect(target.querySelector('audio')?.getAttribute('src')).toBe('blob:test/2'),
    );
    unmount(app);
  });

  it('revokes and removes the player while hidden', async () => {
    const props = $state({ session: session('a'), active: true });
    const { target, app } = await render(props);
    props.active = false;
    flushSync();
    expect(revoke).toHaveBeenCalledWith('blob:test/1');
    expect(target.querySelector('audio')).toBeNull();
    unmount(app);
  });

  it('revokes and removes the player at the retention deadline', async () => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout', 'Date'] });
    const { target, app } = await render({
      session: session('a', Date.now() + 60_000),
      active: true,
    });
    vi.advanceTimersByTime(60_000);
    flushSync();
    expect(revoke).toHaveBeenCalledWith('blob:test/1');
    expect(target.querySelector('audio')).toBeNull();
    unmount(app);
  });
});
