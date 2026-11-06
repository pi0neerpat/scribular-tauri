import type { SessionStatus } from '../types/SessionStatus';

const finished: SessionStatus[] = ['done', 'failed', 'cancelled'];

export function isWorking(status: SessionStatus | null): boolean {
  return status !== null && status !== 'recording' && !finished.includes(status);
}
