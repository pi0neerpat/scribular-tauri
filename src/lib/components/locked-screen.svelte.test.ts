// @vitest-environment jsdom
// R7 dialog focus: opening moves focus to the phrase field; Escape cancels
// and returns focus to the "Start fresh…" button.
import { describe, expect, it, vi } from 'vitest';
import { flushSync, mount, tick, unmount } from 'svelte';

vi.mock('../api', () => ({ quitApp: vi.fn(), startFresh: vi.fn() }));
const { default: LockedScreen } = await import('./LockedScreen.svelte');

const opener = (t: HTMLElement) =>
  [...t.querySelectorAll('button')].find((b) => b.textContent === 'Start fresh…');

describe('LockedScreen focus', () => {
  it('focuses the phrase field, and Escape returns focus to the opener', async () => {
    const target = document.body.appendChild(document.createElement('div'));
    const app = mount(LockedScreen, { target, props: { reason: 'wrongKey' } });
    opener(target)!.click();
    flushSync();
    const input = target.querySelector('input')!;
    expect(document.activeElement).toBe(input);

    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    await tick();
    await tick();
    expect(target.querySelector('[role="dialog"]')).toBeNull();
    expect(document.activeElement).toBe(opener(target));
    unmount(app);
    target.remove();
  });
});
