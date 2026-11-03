import { describe, expect, it } from 'vitest';
import { mergeSnapshot } from './models';
import type { ModelProgress } from './types/ModelProgress';

const m = (modelId: string, state: ModelProgress['state']) => ({ modelId, state }) as ModelProgress;

describe('mergeSnapshot', () => {
  it('keeps event-updated entries over the snapshot', () => {
    expect(mergeSnapshot([m('a', 'missing'), m('b', 'ready')], [m('a', 'ready')])).toEqual([m('b', 'ready'), m('a', 'ready')]);
  });
});
