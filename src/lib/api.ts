// Typed wrappers over Tauri IPC. Components call these, never `invoke` or
// `listen` directly. A Rust test (src-tauri/src/lib.rs) checks that every
// registered command and emitted event has a wrapper here.
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { AppVersion } from './types/AppVersion';
import type { Heartbeat } from './types/Heartbeat';
import type { ModelProgress } from './types/ModelProgress';
import type { ModelSpec } from './types/ModelSpec';
import type { Prompt } from './types/Prompt';
import type { Session } from './types/Session';
import type { SessionListItem } from './types/SessionListItem';
import type { SessionPatch } from './types/SessionPatch';

export function appVersion(): Promise<AppVersion> {
  return invoke<AppVersion>('app_version');
}

export function onHeartbeat(handler: (beat: Heartbeat) => void): Promise<UnlistenFn> {
  return listen<Heartbeat>('app://heartbeat', (event) => handler(event.payload));
}

export function listSessions(): Promise<SessionListItem[]> {
  return invoke<SessionListItem[]>('list_sessions');
}

export function getSession(id: string): Promise<Session | null> {
  return invoke<Session | null>('get_session', { id });
}

export function updateSession(id: string, patch: SessionPatch): Promise<Session> {
  return invoke<Session>('update_session', { id, patch });
}

/** Resolves to whether the session existed. */
export function deleteSession(id: string): Promise<boolean> {
  return invoke<boolean>('delete_session', { id });
}

export function listPrompts(): Promise<Prompt[]> {
  return invoke<Prompt[]>('list_prompts');
}

/** The pinned models with their license terms, in download order. */
export function listModels(): Promise<ModelSpec[]> {
  return invoke<ModelSpec[]>('list_models');
}

export function modelsStatus(): Promise<ModelProgress[]> {
  return invoke<ModelProgress[]>('models_status');
}

/** Starts any missing downloads; progress arrives through `onModelsProgress`. */
export function downloadModels(): Promise<void> {
  return invoke<void>('download_models');
}

export function onModelsProgress(handler: (progress: ModelProgress) => void): Promise<UnlistenFn> {
  return listen<ModelProgress>('models://progress', (event) => handler(event.payload));
}
