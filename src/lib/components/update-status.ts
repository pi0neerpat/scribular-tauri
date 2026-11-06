import type { UpdateState } from '../types/UpdateState';

export function updateStatus(state: UpdateState): string {
  switch (state.state) {
    case 'idle': return 'Up to date';
    case 'checking': return 'Checking…';
    case 'downloading': return state.total ? `Downloading ${Math.floor(state.received * 100 / state.total)}%` : 'Downloading…';
    case 'ready': return `Version ${state.version} ready`;
    case 'error': return `Last check failed: ${state.message}`;
  }
}
