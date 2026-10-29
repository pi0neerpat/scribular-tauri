import { render } from 'svelte/server';
import { describe, expect, it } from 'vitest';
import SummaryPanel from './SummaryPanel.svelte';
import PermissionNotice from './PermissionNotice.svelte';
import RecordView from './RecordView.svelte';

const session = {
  id: 'session-1', title: 'Test', createdAt: 0, inputMode: 'mic' as const,
  status: 'awaitingModel' as const, error: null, promptId: null, modelId: null,
  transcript: 'A saved transcript', summary: '', durationMs: 1000, timings: null,
};

describe('first run components', () => {
  it('shows an AwaitingModel session as waiting while keeping its transcript viewable', () => {
    const { body } = render(SummaryPanel, { props: { session } });
    expect(body).toContain('Waiting for the summary model download');
    expect(body).toContain('A saved transcript');
  });

  it('shows the System Settings action when microphone permission is denied', () => {
    const { body } = render(PermissionNotice, { props: { pane: 'microphone' } });
    expect(body).toContain('Microphone access is off.');
    expect(body).toContain('Open System Settings');
  });

  it('keeps Start recording disabled until the speech model is ready', () => {
    const { body } = render(RecordView, { props: { speechReady: false } });
    expect(body).toMatch(/<button[^>]*disabled[^>]*>Start recording<\/button>/);
  });
});
