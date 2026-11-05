import { render } from 'svelte/server';
import { describe, expect, it } from 'vitest';
import SummaryPanel from './SummaryPanel.svelte';
import PermissionNotice from './PermissionNotice.svelte';
import RecordView from './RecordView.svelte';
import FirstRun from './FirstRun.svelte';
import SpeechModelStatus from './SpeechModelStatus.svelte';
import { isWorking } from './record-status';
import { deniedPrivacy } from '../api';

const session = {
  id: 'session-1', title: 'Test', createdAt: 0, inputMode: 'mic' as const,
  status: 'awaitingModel' as const, error: null, promptId: null, modelId: null,
  transcript: 'A saved transcript', summary: '', durationMs: 1000, timings: null,
  plainTranscript: null, rawTranscript: null, speakerMap: null, diarizationState: 'none' as const,
  hasAudio: false, audioExpiresAt: null,
};

describe('first run components', () => {
  it('shows an AwaitingModel session as waiting while keeping its transcript viewable', () => {
    const { body } = render(SummaryPanel, { props: { session } });
    expect(body).toContain('Waiting for the summary model download');
    expect(body).toContain('A saved transcript');
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
    const { body } = render(RecordView, { props: { speechReady: false } });
    expect(body).toMatch(/<button[^>]*disabled[^>]*>Start recording<\/button>/);
  });

  it('enables Start recording once the speech model is ready', () => {
    const { body } = render(RecordView, { props: { speechReady: true } });
    expect(body).toMatch(/<button(?![^>]*disabled)[^>]*>Start recording<\/button>/);
  });

  it('keeps recording available while a prior session awaits the summary model', () => {
    expect(isWorking('awaitingModel')).toBe(false);
    expect(isWorking('finalizing')).toBe(true);
    expect(isWorking('summarizing')).toBe(true);
  });

  it('shows first-run actions for both macOS permission panes', () => {
    const { body } = render(FirstRun, { props: { speechReady: false } });
    expect(body).toContain('Open Microphone Settings');
    expect(body).toContain('Open System Audio Settings');
  });

  it('shows a failed speech download and an actionable retry', () => {
    const { body } = render(SpeechModelStatus, { props: { speech: {
      modelId: 'whisper-base', state: 'failed', downloadedBytes: 0, totalBytes: 10,
      error: 'network unavailable',
    } } });
    expect(body).toContain('Speech model download failed: network unavailable');
    expect(body).toContain('Retry speech model download');
  });
});
