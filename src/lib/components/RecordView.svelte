<script lang="ts">
  // Records one session with the saved settings, then follows it through
  // transcription and summarization to the result.
  import { onMount } from 'svelte';
  import {
    cancelJob,
    currentRecording,
    deniedPrivacy,
    getRecordingSettings,
    getSession,
    onSessionProgress,
    openPrivacySettings,
    startRecording,
    stopRecording,
  } from '../api';
  import type { Privacy } from '../types/Privacy';
  import type { Session } from '../types/Session';
  import type { SessionProgress } from '../types/SessionProgress';
  import type { SessionStatus } from '../types/SessionStatus';
  import Timings from './Timings.svelte';

  const finished: SessionStatus[] = ['done', 'failed', 'cancelled'];

  let sessionId = $state<string | null>(null);
  // Advanced by start, stop and progress events; never by a fetch, whose
  // reply could be older than the last event.
  let status = $state<SessionStatus | null>(null);
  let progress = $state<SessionProgress | null>(null);
  // The finished session's result.
  let result = $state<Session | null>(null);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let denied = $state<Privacy | null>(null);
  let startedAt = $state(0);
  let now = $state(Date.now());

  let recording = $derived(status === 'recording');
  let working = $derived(status !== null && !recording && !finished.includes(status));
  let elapsed = $derived(Math.max(0, Math.floor((now - startedAt) / 1000)));

  // The clock only ticks during a recording.
  $effect(() => {
    if (!recording) return;
    now = Date.now();
    const tick = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(tick);
  });

  onMount(() => {
    const unlisten = onSessionProgress((p) => {
      if (p.sessionId !== sessionId) return;
      progress = p;
      status = p.state;
      if (finished.includes(p.state)) {
        getSession(p.sessionId).then((s) => (result = s));
      }
    });
    // The window may reload while a recording runs.
    currentRecording().then(async (id) => {
      if (!id) return;
      sessionId = id;
      status = 'recording';
      startedAt = (await getSession(id))?.createdAt ?? Date.now();
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  });

  async function start() {
    busy = true;
    error = denied = null;
    result = progress = null;
    try {
      const settings = await getRecordingSettings();
      const app = settings.inputMode === 'mic' ? null : settings.targetApp;
      const title = `Session ${new Date().toLocaleString()}`;
      const session = await startRecording(settings.inputMode, app, title);
      sessionId = session.id;
      status = session.status;
      startedAt = Date.now();
    } catch (e) {
      denied = deniedPrivacy(String(e));
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function stop() {
    busy = true;
    try {
      await stopRecording();
      // Its "Transcribing…" event may already have moved it on.
      if (status === 'recording') status = 'finalizing';
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function cancel() {
    if (sessionId) await cancelJob(sessionId);
  }
</script>

<section>
  <h2>Record</h2>
  {#if recording}
    <p>Recording… {Math.floor(elapsed / 60)}:{String(elapsed % 60).padStart(2, '0')}</p>
    <button onclick={stop} disabled={busy}>Stop</button>
  {:else if working}
    <p>
      {progress?.message ?? 'Working…'}
      {#if progress && progress.stepCount > 1}({progress.stepIndex}/{progress.stepCount}){/if}
    </p>
    <button onclick={cancel}>Cancel</button>
  {:else}
    <button onclick={start} disabled={busy}>Start recording</button>
  {/if}

  {#if denied}
    <p role="alert">
      {denied === 'microphone' ? 'Microphone' : 'App audio'} access is off.
      <button onclick={() => openPrivacySettings(denied!)}>Open System Settings</button>
    </p>
  {:else if error}
    <p role="alert">{error}</p>
  {/if}

  {#if result}
    {@const session = result}
    <article>
      <h3>{session.title}</h3>
      {#if session.status === 'failed'}
        <p role="alert">{session.error}</p>
      {:else if session.status === 'cancelled'}
        <p>Stopped.</p>
      {/if}
      {#if session.summary}
        <h4>Summary</h4>
        <pre>{session.summary}</pre>
      {/if}
      {#if session.transcript}
        <h4>Transcript</h4>
        <pre>{session.transcript}</pre>
      {/if}
      {#if session.timings}
        <Timings timings={session.timings} />
      {/if}
    </article>
  {/if}
</section>

<style>
  pre {
    white-space: pre-wrap;
  }
</style>
