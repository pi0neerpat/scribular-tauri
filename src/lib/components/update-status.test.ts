import { describe, expect, it } from 'vitest';
import { updateStatus } from './update-status';

describe('updateStatus', () => {
  it('covers every updater state and formats download progress', () => {
    expect(updateStatus({ state: 'idle', installable: false })).toBe('Up to date');
    expect(updateStatus({ state: 'checking', installable: false })).toBe('Checking…');
    expect(updateStatus({ state: 'downloading', installable: false, received: 40, total: 100 })).toBe('Downloading 40%');
    expect(updateStatus({ state: 'ready', version: '0.2.0', notes: '', installable: true })).toBe('Version 0.2.0 ready');
    expect(updateStatus({ state: 'error', installable: false, message: 'offline' })).toBe('Last check failed: offline');
  });
});
