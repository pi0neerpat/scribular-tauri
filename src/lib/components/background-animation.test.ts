import { describe, expect, it } from 'vitest';
import { starsState } from './background-animation';
import type { Background } from '../types/Background';

// AS8
describe('Stars motion', () => {
  it('animates only when Stars motion is allowed and speeds up while recording', () => {
    for (const motion of [true, false]) for (const recording of [true, false]) for (const hidden of [true, false]) for (const reduced of [true, false]) {
      const state = starsState('stars', motion, recording, hidden, reduced);
      expect(state.canvas).toBe(true);
      expect(state.animate).toBe(motion && !hidden && !reduced);
      expect(state.speed).toBe(recording ? .0018 : .00045);
    }
    expect(starsState('stars', true, true, false, false).speed).toBeGreaterThan(starsState('stars', true, false, false, false).speed);
  });

  it('never keeps a star canvas for other pictures', () => {
    const others: Background[] = ['none', 'dawn', 'seaGlass', 'linen', { owned: 'abc123' }];
    for (const background of others) {
      expect(starsState(background, true, true, false, false)).toMatchObject({ canvas: false, animate: false });
    }
  });
});
