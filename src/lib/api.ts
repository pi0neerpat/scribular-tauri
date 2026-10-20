// Typed wrappers over Tauri IPC. Components call these, never `invoke` or
// `listen` directly. A Rust test (src-tauri/src/lib.rs) checks that every
// registered command and emitted event has a wrapper here.
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { AppVersion } from './types/AppVersion';
import type { AudioApp } from './types/AudioApp';
import type { Heartbeat } from './types/Heartbeat';
import type { InputMode } from './types/InputMode';
import type { ModelProgress } from './types/ModelProgress';
import type { ModelSpec } from './types/ModelSpec';
import type { Privacy } from './types/Privacy';
import type { Prompt } from './types/Prompt';
import type { RecordingSettings } from './types/RecordingSettings';
import type { Session } from './types/Session';
import type { SessionListItem } from './types/SessionListItem';
import type { SessionPatch } from './types/SessionPatch';
import type { SessionProgress } from './types/SessionProgress';
import type { SummaryDelta } from './types/SummaryDelta';
import type { TranscriptSegment } from './types/TranscriptSegment';

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

/** Queues a fresh summary; progress arrives through `onSessionProgress`. */
export function regenerateSummary(sessionId: string, promptId: string): Promise<void> {
  return invoke<void>('regenerate_summary', { sessionId, promptId });
}

/** Stops the session's transcription or summary; resolves to whether it had either. */
export function cancelJob(sessionId: string): Promise<boolean> {
  return invoke<boolean>('cancel_job', { sessionId });
}

/** Development only: transcribes a 16-bit, 16 kHz mono WAV file. */
export function transcribeFile(path: string): Promise<TranscriptSegment[]> {
  return invoke<TranscriptSegment[]>('transcribe_file', { path });
}

export function getRecordingSettings(): Promise<RecordingSettings> {
  return invoke<RecordingSettings>('get_recording_settings');
}

export function setRecordingSettings(settings: RecordingSettings): Promise<void> {
  return invoke<void>('set_recording_settings', { settings });
}

/** Apps playing audio right now, for the system-audio picker. */
export function listAudioApps(): Promise<AudioApp[]> {
  return invoke<AudioApp[]>('list_audio_apps');
}

/**
 * Starts recording a new session. `appId` is required unless `mode` is
 * `mic`. A refused macOS permission rejects with a message that
 * `deniedPrivacy` recognizes.
 */
export function startRecording(
  mode: InputMode,
  appId: string | null,
  title: string,
): Promise<Session> {
  return invoke<Session>('start_recording', { mode, appId, title });
}

/** Stops recording and starts transcription; resolves to the session id. */
export function stopRecording(): Promise<string> {
  return invoke<string>('stop_recording');
}

/** The privacy pane a "permission denied" error points to, if it is one. */
export function deniedPrivacy(error: string): Privacy | null {
  if (/microphone permission denied/i.test(error)) return 'microphone';
  if (/system audio recording permission denied/i.test(error)) return 'systemAudio';
  return null;
}

/** Opens the System Settings pane that grants `pane`'s permission. */
export function openPrivacySettings(pane: Privacy): Promise<void> {
  return invoke<void>('open_privacy_settings', { pane });
}

/** The id of the session being recorded, if any. */
export function currentRecording(): Promise<string | null> {
  return invoke<string | null>('current_recording');
}

export function onSessionProgress(
  handler: (progress: SessionProgress) => void,
): Promise<UnlistenFn> {
  return listen<SessionProgress>('session://progress', (event) => handler(event.payload));
}

/** Text of the final summary pass as it decodes. */
export function onSummaryDelta(handler: (delta: SummaryDelta) => void): Promise<UnlistenFn> {
  return listen<SummaryDelta>('session://summary-delta', (event) => handler(event.payload));
}
