import { expect, it } from 'vitest';
import { shouldAnimate } from './background-animation';

it('shouldAnimate returns whether frames may run', () => {
  expect(shouldAnimate(false, false, false)).toBe(false);
  expect(shouldAnimate(true, false, false)).toBe(true);
  expect(shouldAnimate(true, true, false)).toBe(false);
  expect(shouldAnimate(true, false, true)).toBe(false);
});
