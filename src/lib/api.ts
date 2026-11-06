// Typed wrappers over Tauri IPC. Components call these, never `invoke` or
// `listen` directly. A Rust test (src-tauri/src/lib.rs) checks that every
// registered command and emitted event has a wrapper here.
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { AccessState } from './types/AccessState';
import type { ActivationResult } from './types/ActivationResult';
import type { AppSettings } from './types/AppSettings';
import type { AppVersion } from './types/AppVersion';
import type { AudioApp } from './types/AudioApp';
import type { Heartbeat } from './types/Heartbeat';
import type { InputMode } from './types/InputMode';
import type { LockReason } from './types/LockReason';
import type { ModelProgress } from './types/ModelProgress';
import type { ModelSpec } from './types/ModelSpec';
import type { Privacy } from './types/Privacy';
import type { Prompt } from './types/Prompt';
import type { PromptList } from './types/PromptList';
import type { RecordingSettings } from './types/RecordingSettings';
import type { Session } from './types/Session';
import type { SessionListItem } from './types/SessionListItem';
import type { SessionPatch } from './types/SessionPatch';
import type { SessionProgress } from './types/SessionProgress';
import type { SessionSegments } from './types/SessionSegments';
import type { SummaryDelta } from './types/SummaryDelta';
import type { SummaryModel } from './types/SummaryModel';
import type { TranscriptSegment } from './types/TranscriptSegment';
import type { UpdateState } from './types/UpdateState';

type SpeakerMap = Record<string, string>;

export function appVersion(): Promise<AppVersion> {
  return invoke<AppVersion>('app_version');
}

export function checkForUpdates(): Promise<void> { return invoke<void>('check_for_updates'); }
export function getUpdateState(): Promise<UpdateState> { return invoke<UpdateState>('get_update_state'); }
export function installUpdate(): Promise<void> { return invoke<void>('install_update'); }
export function onUpdateState(handler: (state: UpdateState) => void): Promise<UnlistenFn> {
  return listen<UpdateState>('update://state', (event) => handler(event.payload));
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

/**
 * A session's kept audio as WAV bytes. Rejects with a `PlaybackError`
 * (`'notFound' | 'expired' | 'tooLarge' | 'failed'`).
 */
export async function requestSessionAudio(id: string): Promise<Uint8Array<ArrayBuffer>> {
  // The macOS webview can deliver a raw IPC Response as a plain number[] (seen in the
  // release app); `new Blob([number[]])` would turn it into text, so normalise to bytes.
  return new Uint8Array(await invoke<ArrayBuffer | number[]>('request_session_audio', { id }));
}

/** Why the store couldn't be opened, or null when it did. Ask this first. */
export function lockedState(): Promise<LockReason | null> {
  return invoke<LockReason | null>('locked_state');
}

/** Sets the unreadable database aside and restarts with an empty one. */
export function startFresh(confirmation: string): Promise<void> {
  return invoke<void>('start_fresh', { confirmation });
}

export function quitApp(): Promise<void> {
  return invoke<void>('quit_app');
}
export function confirmQuit(): Promise<void> { return invoke<void>('confirm_quit'); }
export function notifyTrayStartFailure(message: string): Promise<void> { return invoke<void>('notify_tray_start_failure', { message }); }
export function onTrayStartRecording(handler: () => void): Promise<UnlistenFn> {
  return listen('tray-start-recording', () => handler());
}
export function onQuitRequested(handler: () => void): Promise<UnlistenFn> {
  return listen('quit-requested', () => handler());
}

/** True once, after the prototype database was encrypted. */
export function takeEncryptionNotice(): Promise<boolean> {
  return invoke<boolean>('take_encryption_notice');
}

/** Resolves to whether the session existed. */
export function deleteSession(id: string): Promise<boolean> {
  return invoke<boolean>('delete_session', { id });
}

export function listPrompts(): Promise<Prompt[]> {
  return invoke<Prompt[]>('list_prompts');
}

/** Every style plus the active id. The style commands below return the same. */
export function getPromptList(): Promise<PromptList> {
  return invoke<PromptList>('get_prompt_list');
}

export function createPrompt(name: string, text: string): Promise<PromptList> {
  return invoke<PromptList>('create_prompt', { name, text });
}

export function duplicatePrompt(id: string): Promise<PromptList> {
  return invoke<PromptList>('duplicate_prompt', { id });
}

export function updatePrompt(id: string, name: string, text: string): Promise<PromptList> {
  return invoke<PromptList>('update_prompt', { id, name, text });
}

export function deletePrompt(id: string): Promise<PromptList> {
  return invoke<PromptList>('delete_prompt', { id });
}

export function setPromptStarred(id: string, starred: boolean): Promise<PromptList> {
  return invoke<PromptList>('set_prompt_starred', { id, starred });
}

/** Makes `id` the style new sessions use; stars it if needed. */
export function setActivePrompt(id: string): Promise<PromptList> {
  return invoke<PromptList>('set_active_prompt', { id });
}

export function listSummaryModels(): Promise<SummaryModel[]> {
  return invoke<SummaryModel[]>('list_summary_models');
}

/** Switches the summary model; a missing one downloads via `onModelsProgress`. */
export function setSummaryModel(modelId: string): Promise<void> {
  return invoke<void>('set_summary_model', { modelId });
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

export function downloadModel(modelId: string): Promise<void> {
  return invoke<void>('download_model', { modelId });
}

export function confirmSpeakerLabels(id: string, speakerMap: SpeakerMap): Promise<string> {
  return invoke<string>('confirm_speaker_labels', { id, speakerMap });
}

export function skipDiarization(id: string): Promise<string> {
  return invoke<string>('skip_diarization', { id });
}

export function updateSpeakerMap(id: string, speakerMap: SpeakerMap): Promise<string> {
  return invoke<string>('update_speaker_map', { id, speakerMap });
}

export function onModelsProgress(handler: (progress: ModelProgress) => void): Promise<UnlistenFn> {
  return listen<ModelProgress>('models://progress', (event) => handler(event.payload));
}

export function onSummaryModelChanged(handler: (modelId: string) => void): Promise<UnlistenFn> {
  return listen<string>('summary-model://changed', (event) => handler(event.payload));
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

export function getAppSettings(): Promise<AppSettings> { return invoke<AppSettings>('get_app_settings'); }
export function setAppSettings(settings: AppSettings): Promise<void> { return invoke<void>('set_app_settings', { settings }); }
export function requestNotificationPermission(): Promise<boolean> { return invoke<boolean>('request_notification_permission'); }

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
export function stopRecording(numSpeakers: number | null = null): Promise<string> {
  return invoke<string>('stop_recording', { numSpeakers });
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

/** New transcript segments while a session records. */
export function onSessionSegments(
  handler: (segments: SessionSegments) => void,
): Promise<UnlistenFn> {
  return listen<SessionSegments>('session://segments', (event) => handler(event.payload));
}

/** Trial or licence state, from the last check; changes arrive through `onLicenseChanged`. */
export function getAccessState(): Promise<AccessState> {
  return invoke<AccessState>('get_access_state');
}

/** The saved key as `abcd***wxyz`, or null. */
export function getMaskedLicenseKey(): Promise<string | null> {
  return invoke<string | null>('get_masked_license_key');
}

export function activateLicense(key: string): Promise<ActivationResult> {
  return invoke<ActivationResult>('activate_license', { key });
}

/** Forgets the key on this Mac. No network call. */
export function deactivateLicense(): Promise<void> {
  return invoke<void>('deactivate_license');
}

/** Opens an http, https or mailto link in the default app. */
export function openExternalLink(url: string): Promise<void> {
  return invoke<void>('open_external_link', { url });
}

export function onLicenseChanged(handler: (state: AccessState) => void): Promise<UnlistenFn> {
  return listen<AccessState>('license://changed', (event) => handler(event.payload));
}

/** The access state inside a refused `startRecording` or `transcribeFile`, if that is why it failed. */
export function accessDenied(error: unknown): AccessState | null {
  try {
    const parsed = JSON.parse(String(error)) as { kind?: string; state: AccessState };
    return parsed?.kind === 'accessDenied' ? parsed.state : null;
  } catch {
    return null;
  }
}
