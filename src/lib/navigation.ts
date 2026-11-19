import { writable } from 'svelte/store';
import type { ModelProgress } from './types/ModelProgress';

export type AppView = 'record' | 'history' | 'styles';
export const navigation = writable({ view: 'record' as AppView, navigationLocked: false });

export function navigationLockedBy(models: ModelProgress[]): boolean {
  return models.some((m) => m.state === 'downloading' || m.state === 'verifying');
}

export function navigationDisabled(locked: boolean, target: AppView, current: AppView): boolean {
  return locked && target !== 'styles' && !(target === 'record' && current === 'styles');
}
