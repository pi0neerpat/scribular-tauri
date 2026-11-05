// followAccessState: an event that lands before the snapshot resolves wins,
// and a failed listener registration still loads the snapshot.
import { afterEach, describe, expect, it, vi } from 'vitest';
import { get } from 'svelte/store';
import type { AccessState } from './types/AccessState';

const getAccessState = vi.fn<() => Promise<AccessState>>();
const onLicenseChanged = vi.fn<(h: (s: AccessState) => void) => Promise<() => void>>();
vi.mock('./api', () => ({ getAccessState, onLicenseChanged }));
const { accessState, followAccessState } = await import('./license');

const flush = () => new Promise((r) => setTimeout(r, 0));

afterEach(() => {
  accessState.set(null);
  vi.resetAllMocks();
});

describe('followAccessState', () => {
  it('does not let a stale snapshot overwrite an earlier event', async () => {
    let emit!: (s: AccessState) => void;
    let resolveSnapshot!: (s: AccessState) => void;
    onLicenseChanged.mockImplementation((h) => {
      emit = h;
      return Promise.resolve(() => {});
    });
    getAccessState.mockReturnValue(new Promise((r) => (resolveSnapshot = r)));

    followAccessState();
    await flush();
    emit({ kind: 'licensed', expiresAt: null, verifiedAt: 1 });
    resolveSnapshot({ kind: 'trialEnded' });
    await flush();

    expect(get(accessState)).toEqual({
      kind: 'licensed',
      expiresAt: null,
      verifiedAt: 1,
    });
  });

  it('still fetches the snapshot when listener registration rejects', async () => {
    onLicenseChanged.mockReturnValue(Promise.reject(new Error('no ipc')));
    getAccessState.mockResolvedValue({ kind: 'trialEnded' });

    const stop = followAccessState();
    await flush();

    expect(get(accessState)).toEqual({ kind: 'trialEnded' });
    stop();
  });
});
