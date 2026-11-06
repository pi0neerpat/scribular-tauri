import { expect, it } from 'vitest';
import { navigationDisabled, navigationLockedBy } from './navigation';
import type { ModelProgress } from './types/ModelProgress';

const model = (state: ModelProgress['state']): ModelProgress => ({ modelId: 'm', state, downloadedBytes: 0, totalBytes: 0, error: null });

it('locks navigation only while a model downloads or verifies', () => {
  expect(navigationLockedBy([model('downloading')])).toBe(true);
  expect(navigationLockedBy([model('verifying')])).toBe(true);
  expect(navigationLockedBy([model('ready')])).toBe(false);
  expect(navigationLockedBy([model('missing')])).toBe(false);
  expect(navigationLockedBy([model('failed')])).toBe(false);
  expect(navigationDisabled(false)).toBe(false);
  expect(navigationDisabled(true)).toBe(true);
});
