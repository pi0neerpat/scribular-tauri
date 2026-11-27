import type { Background } from '../types/Background';

export type StarsState = { canvas: boolean; animate: boolean; speed: number };

// Theme R8: only Stars mounts the canvas. Its motion switch, a hidden window
// and macOS Reduce Motion all keep it still; recording speeds it up.
export function starsState(background: Background, motion: boolean, recording: boolean, hidden: boolean, reducedMotion: boolean): StarsState {
  const canvas = background === 'stars';
  return { canvas, animate: canvas && motion && !hidden && !reducedMotion, speed: recording ? .0018 : .00045 };
}
