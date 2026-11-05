import { render } from 'svelte/server';
import { describe, expect, it, vi } from 'vitest';
import SpeakersCard from './SpeakersCard.svelte';
import type { Session } from '../types/Session';

vi.mock('../api', () => ({
  confirmSpeakerLabels: vi.fn(),
  getSession: vi.fn(),
  skipDiarization: vi.fn(),
  updateSpeakerMap: vi.fn(),
}));

const names = Object.fromEntries(Array.from({ length: 6 }, (_, i) => [`SPEAKER_${i}`, `Speaker ${i + 1}`]));
const session: Session = {
  id: 'session-1', title: 'Test', createdAt: 0, inputMode: 'mic', status: 'done', error: null,
  promptId: null, modelId: null, transcript: '', plainTranscript: 'plain',
  rawTranscript: Object.keys(names).map((id) => `${id}: hello`).join('\n'), speakerMap: names,
  diarizationState: 'pending', summary: '', durationMs: 1000, timings: null, hasAudio: false,
  audioExpiresAt: null,
};

describe('speaker assignment card', () => {
  it('shows speaker quotes, typed-name chips, confirm/skip actions, and cycles five colours', () => {
    const { body } = render(SpeakersCard, { props: { session } });
    expect(body).toContain('hello');
    expect(body).toContain('Speaker 6');
    expect(body).toContain('Confirm speakers');
    expect(body).toContain('Skip');
    for (let i = 0; i < 5; i++) expect(body).toContain(`color-${i}`);
  });
});
