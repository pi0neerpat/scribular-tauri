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

export const SPEEDS = [0.75, 1, 1.25, 1.5, 2];

/** Index of the line playing at `ms`: the last one that has started, or -1 before the first. */
export function currentLine(lines: { startMs: number }[], ms: number): number {
  let found = -1;
  for (let i = 0; i < lines.length && lines[i].startMs <= ms; i++) found = i;
  return found;
}

/** "1:05" from seconds. */
export function formatTime(seconds: number): string {
  const s = Math.max(0, Math.floor(seconds || 0));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
}

/** Why the Audio tab is disabled (R15), or null when audio can play. */
export function audioUnavailable(session: { hasAudio: boolean; audioExpiresAt: number | null }, now: number = Date.now()): string | null {
  if (session.audioExpiresAt !== null && now >= session.audioExpiresAt) return "This session's audio was deleted after seven days. The note and transcript are still here.";
  if (!session.hasAudio) return 'No audio was kept for this session. Turn on Keep audio in Settings to keep future recordings on this Mac for seven days.';
  return null;
}
