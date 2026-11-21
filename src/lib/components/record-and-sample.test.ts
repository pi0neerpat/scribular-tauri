import { render } from 'svelte/server';
import { describe, expect, it } from 'vitest';
import SummaryPanel from './SummaryPanel.svelte';
import PermissionNotice from './PermissionNotice.svelte';
import RecordView from './RecordView.svelte';
import SampleBadge from './SampleBadge.svelte';
import SampleBanner from './SampleBanner.svelte';
import { isWorking } from './record-status';
import { deniedPrivacy } from '../api';

const session = {
  id: 'session-1', title: 'Test', createdAt: 0, inputMode: 'mic' as const,
  status: 'awaitingModel' as const, error: null, promptId: null, modelId: null,
  transcript: 'A saved transcript', summary: '', durationMs: 1000, timings: null,
  plainTranscript: null, rawTranscript: null, speakerMap: null, diarizationState: 'none' as const,
  hasAudio: false, isSample: false, audioExpiresAt: null, previousSummary: null, previousPromptId: null, previousModelId: null, canRedo: false,
};

describe('recording and sample components', () => {
  it('shows an AwaitingModel session as waiting while keeping its transcript viewable', () => {
    const { body } = render(SummaryPanel, { props: { session } });
    expect(body).toContain('Waiting for the summary model download');
    expect(body).toContain('A saved transcript');
  });

  it('keeps the download quiet in a completed note', () => {
    const { body } = render(SummaryPanel, { props: { session: { ...session, status: 'done', summary: 'A saved note' } } });
    expect(body).toContain('A saved note');
    expect(body).not.toContain('Waiting for the summary model download');
  });

  it('shows the System Settings action when microphone permission is denied', () => {
    const pane = deniedPrivacy('microphone permission denied');
    expect(pane).toBe('microphone');
    if (!pane) throw new Error('denied microphone was not recognized');
    const { body } = render(PermissionNotice, { props: { pane } });
    expect(body).toContain('Microphone access is off.');
    expect(body).toContain('Open System Settings');
  });

  it('keeps Start recording disabled until the speech model is ready', () => {
    const { body } = render(RecordView, { props: {} });
    expect(body).toMatch(/<button[^>]*disabled[^>]*>\s*Start recording\s*<\/button>/);
  });

  it('waits for saved source before enabling Start', () => {
    const { body } = render(RecordView, { props: { speech: { modelId: 'whisper-base', state: 'ready', downloadedBytes: 100, totalBytes: 100, error: null } } });
    expect(body).toMatch(/<button[^>]*disabled[^>]*>\s*Start recording\s*<\/button>/);
  });

  it('allows cancelling while a session awaits the summary model', () => {
    expect(isWorking('awaitingModel')).toBe(true);
    expect(isWorking('finalizing')).toBe(true);
    expect(isWorking('summarizing')).toBe(true);
  });

  it('provides a labelled sample badge and example banner', () => {
    expect(render(SampleBadge).body).toContain('Sample');
    expect(render(SampleBanner).body).toContain('This is an example. It is not a real recording.');
  });
});
