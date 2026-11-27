// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';
import { get } from 'svelte/store';
import ThemeBackground from './ThemeBackground.svelte';
import { appearanceError, PICTURE_UNAVAILABLE } from '../appearance-store';
import type { Background } from '../types/Background';

let frames = 0;
let queued = new Map<number, FrameRequestCallback>();
let cancelled: number[] = [];
let painted = 0;
// Runs the frames queued so far, as the browser would on the next paint.
function tick() {
  const due = [...queued.values()];
  queued = new Map();
  for (const callback of due) callback(0);
}
beforeEach(() => {
  frames = 0;
  queued = new Map();
  cancelled = [];
  painted = 0;
  vi.stubGlobal('matchMedia', () => ({ matches: false, addEventListener: () => {}, removeEventListener: () => {} }));
  vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => { queued.set(++frames, callback); return frames; });
  vi.stubGlobal('cancelAnimationFrame', (id: number) => { cancelled.push(id); queued.delete(id); });
  vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue({ clearRect: () => { painted++; }, fillRect: () => {}, fillStyle: '' } as unknown as CanvasRenderingContext2D);
});
afterEach(() => { vi.unstubAllGlobals(); vi.restoreAllMocks(); });

function render(background: Background, motion = true) {
  const target = document.createElement('div');
  const props = $state({ background, motion, recording: false, blur: 20, brightness: 80 });
  const app = mount(ThemeBackground, { target, props });
  flushSync();
  return { target, props, app };
}

// AS8: switching away from Stars removes the canvas and its animation.
it('mounts the star canvas only for Stars and runs frames only with motion on', () => {
  const { target, props, app } = render('stars');
  expect(target.querySelector('canvas')).not.toBeNull();
  expect(queued.size).toBe(1);
  // Each frame draws and schedules exactly one next frame.
  const before = painted;
  tick();
  tick();
  expect(painted).toBe(before + 2);
  expect(queued.size).toBe(1);
  const pending = [...queued.keys()][0];
  props.background = 'dawn';
  flushSync();
  expect(target.querySelector('canvas')).toBeNull();
  expect(cancelled).toContain(pending);
  expect(queued.size).toBe(0);
  tick();
  expect(queued.size).toBe(0);
  unmount(app);

  frames = 0;
  const still = render('stars', false);
  expect(still.target.querySelector('canvas')).not.toBeNull();
  expect(frames).toBe(0);
  unmount(still.app);
});

it('never starts the frame loop without a 2D context', () => {
  vi.mocked(HTMLCanvasElement.prototype.getContext).mockReturnValue(null);
  const { target, props, app } = render('stars');
  expect(target.querySelector('canvas')).not.toBeNull();
  props.motion = false;
  flushSync();
  props.motion = true;
  flushSync();
  expect(queued.size).toBe(0);
  unmount(app);
});

it('tells the user when the owned picture cannot be shown, but not for a stale picture', () => {
  appearanceError.set(null);
  // Keep the first picture's error handler: {#key} removes it with the old <img>.
  const handlers = new Map<EventTarget, EventListener>();
  const add = HTMLImageElement.prototype.addEventListener;
  vi.spyOn(HTMLImageElement.prototype, 'addEventListener').mockImplementation(function (this: HTMLImageElement, type: string, listener: EventListenerOrEventListenerObject, options?: boolean | AddEventListenerOptions) {
    if (type === 'error') handlers.set(this, listener as EventListener);
    add.call(this, type, listener, options);
  });
  const { target, props, app } = render({ owned: 'abc123' });
  const old = target.querySelector('img')!;
  const oldError = handlers.get(old)!;
  expect(oldError).toBeDefined();
  props.background = { owned: 'def456' };
  flushSync();
  // A late error from the previous picture leaves the new one on screen.
  add.call(old, 'error', oldError);
  old.dispatchEvent(new Event('error'));
  flushSync();
  expect(target.querySelector('img')?.getAttribute('src')).toBe('scribular-bg://localhost/def456');
  expect(get(appearanceError)).toBeNull();
  target.querySelector('img')!.dispatchEvent(new Event('error'));
  flushSync();
  expect(target.querySelector('img')).toBeNull();
  expect(get(appearanceError)).toBe(PICTURE_UNAVAILABLE);
  unmount(app);
});

it('shows bundled and owned pictures from allowed sources and falls back to solid on error', () => {
  const { target, props, app } = render({ owned: 'abc123' });
  const img = target.querySelector('img')!;
  expect(img.getAttribute('src')).toBe('scribular-bg://localhost/abc123');
  expect(img.style.filter).toBe('blur(6px) brightness(80%)');
  img.dispatchEvent(new Event('error'));
  flushSync();
  expect(target.querySelector('img')).toBeNull();
  props.background = 'linen';
  flushSync();
  expect(target.querySelector('img')?.getAttribute('src')).toMatch(/linen.*\.jpg$/);
  props.background = 'none';
  flushSync();
  expect(target.querySelector('img, canvas')).toBeNull();
  unmount(app);
});
