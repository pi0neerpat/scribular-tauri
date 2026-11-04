// Kept-audio playback (parity spec 01 R19/R22): what the player shows.
import { requestSessionAudio } from '../api';
import type { PlaybackError } from '../types/PlaybackError';

export type AudioView =
  | { kind: 'hidden' }
  | { kind: 'player'; url: string }
  | { kind: 'message'; text: string };

const messages: Record<PlaybackError, string | null> = {
  notFound: null,
  expired: 'This recording has been deleted',
  tooLarge: 'Recording too long to play in the app',
  failed: "Couldn't load the recording",
};

/** Fetches the WAV and wraps it in a blob URL; the caller revokes it. Never throws. */
export async function openAudio(id: string): Promise<AudioView> {
  try {
    const bytes = await requestSessionAudio(id);
    return { kind: 'player', url: URL.createObjectURL(new Blob([bytes], { type: 'audio/wav' })) };
  } catch (e) {
    const text = typeof e === 'string' && e in messages ? messages[e as PlaybackError] : messages.failed;
    return text === null ? { kind: 'hidden' } : { kind: 'message', text };
  }
}

/** "Recording deletes Nov 11", or "Recording deletes today" on the last day. */
export function retentionLine(expiresAt: number, now: number = Date.now()): string {
  const at = new Date(expiresAt);
  if (at.toDateString() === new Date(now).toDateString() || expiresAt < now) {
    return 'Recording deletes today';
  }
  return `Recording deletes ${at.toLocaleDateString('en-US', { month: 'short', day: 'numeric' })}`;
}
