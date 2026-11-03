import type { ModelProgress } from './types/ModelProgress';

// Entries already updated by a progress event are newer than the snapshot.
export function mergeSnapshot(snapshot: ModelProgress[], current: ModelProgress[]): ModelProgress[] {
  return [...snapshot.filter((s) => !current.some((m) => m.modelId === s.modelId)), ...current];
}
