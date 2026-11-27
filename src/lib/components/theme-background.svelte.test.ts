// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';
import ThemeBackground from './ThemeBackground.svelte';
import type { Background } from '../types/Background';

let frames = 0;
beforeEach(() => {
  frames = 0;
  vi.stubGlobal('matchMedia', () => ({ matches: false, addEventListener: () => {}, removeEventListener: () => {} }));
  vi.stubGlobal('requestAnimationFrame', () => ++frames);
  vi.stubGlobal('cancelAnimationFrame', () => {});
  vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue({ clearRect: () => {}, fillRect: () => {}, fillStyle: '' } as unknown as CanvasRenderingContext2D);
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
  expect(frames).toBeGreaterThan(0);
  props.background = 'dawn';
  flushSync();
  expect(target.querySelector('canvas')).toBeNull();
  unmount(app);

  frames = 0;
  const still = render('stars', false);
  expect(still.target.querySelector('canvas')).not.toBeNull();
  expect(frames).toBe(0);
  unmount(still.app);
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
