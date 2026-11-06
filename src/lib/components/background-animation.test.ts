import { expect, it } from 'vitest';
import { shouldAnimate } from './background-animation';

it('stops frames when disabled and resumes when turned back on', () => {
  expect(shouldAnimate(false, false, false)).toBe(false);
  expect(shouldAnimate(true, false, false)).toBe(true);
  expect(shouldAnimate(true, true, false)).toBe(false);
  expect(shouldAnimate(true, false, true)).toBe(false);
});
