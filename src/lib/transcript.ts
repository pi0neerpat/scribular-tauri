// The transcript as it records, built like the Rust core's final one
// (src-tauri/src/text/merge.rs): two sources become speaker turns, one
// source is its segments joined.
import type { AudioSource } from './types/AudioSource';
import type { TranscriptSegment } from './types/TranscriptSegment';

export type SourceSegments = Record<AudioSource, TranscriptSegment[]>;

const speakers: [AudioSource, string][] = [
  ['system', 'Client'],
  ['mic', 'You'],
];

export function liveTranscript(segments: SourceSegments, labeled: boolean): string {
  if (!labeled) {
    return [...segments.system, ...segments.mic]
      .map((s) => s.text.trim())
      .filter(Boolean)
      .join(' ');
  }
  // Array.prototype.sort is stable: ties keep Client before You.
  const turns = speakers
    .flatMap(([source, name]) => segments[source].map((s) => ({ name, s })))
    .sort((a, b) => a.s.startMs - b.s.startMs);
  const lines: { name: string; texts: string[] }[] = [];
  for (const { name, s } of turns) {
    const text = s.text.replace(/\[[^\]]*\]/g, '').trim();
    if (!text) continue;
    const last = lines.at(-1);
    if (last?.name === name) last.texts.push(text);
    else lines.push({ name, texts: [text] });
  }
  return lines.map((l) => `${l.name}: ${l.texts.join(' ')}`).join('\n');
}
