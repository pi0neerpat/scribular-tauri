import { describe, expect, it } from 'vitest';
import { mergeSnapshot } from './models';
import type { ModelProgress } from './types/ModelProgress';

const m = (modelId: string, state: ModelProgress['state'], downloadedBytes = 0, error: string | null = null): ModelProgress => ({
  modelId,
  state,
  downloadedBytes,
  totalBytes: 100,
  error,
});

describe('mergeSnapshot', () => {
  it('keeps event-updated entries over the snapshot', () => {
    const snapshot = [m('a', 'missing'), m('b', 'ready', 100)];
    const current = [m('a', 'failed', 42, 'checksum mismatch')];
    expect(mergeSnapshot(snapshot, current)).toEqual([m('b', 'ready', 100), m('a', 'failed', 42, 'checksum mismatch')]);
  });
});
