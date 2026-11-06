<script lang="ts">
  // Records one session with the saved settings, then follows it through
  // transcription and summarization to the result.
  import { onMount, tick } from 'svelte';
  import {
    accessDenied,
    cancelJob,
    currentRecording,
    deniedPrivacy,
    getPromptList,
    getRecordingSettings,
    getSession,
    listSummaryModels,
    onSessionProgress,
    setActivePrompt,
    startRecording,
    stopRecording,
  } from '../api';
  import type { Privacy } from '../types/Privacy';
  import type { PromptList } from '../types/PromptList';
  import type { Session } from '../types/Session';
  import type { SessionProgress } from '../types/SessionProgress';
  import type { SessionStatus } from '../types/SessionStatus';
  import LiveTranscript from './LiveTranscript.svelte';
  import Timings from './Timings.svelte';
  import SummaryPanel from './SummaryPanel.svelte';
  import StylePicker from './StylePicker.svelte';
  import PermissionNotice from './PermissionNotice.svelte';
  import { isWorking } from './record-status';
  import LicenseGate from './LicenseGate.svelte';
  import { accessState, allows } from '../license';

  let {
    speechReady = false,
    active = true,
    onEnterLicense = () => {},
  }: { speechReady: boolean; active?: boolean; onEnterLicense?: () => void } = $props();

  const finished: SessionStatus[] = ['done', 'failed', 'cancelled'];
  // The session in flight, kept across a window reload. The backend reports
  // only the one recording, not a session after Stop. A window session, not
  // local storage, so a crashed app's session isn't picked up on relaunch.
  const IN_FLIGHT_KEY = 'scribular.inFlightSession';
  const IN_FLIGHT_SPEAKERS_KEY = 'scribular.inFlightMultiSpeaker';
  const IN_FLIGHT_START_KEY = 'scribular.inFlightStartAttempt';

  function remember(id: string | null) {
    if (id) sessionStorage.setItem(IN_FLIGHT_KEY, id);
    else {
      sessionStorage.removeItem(IN_FLIGHT_KEY);
      if (!sessionStorage.getItem(IN_FLIGHT_START_KEY)) sessionStorage.removeItem(IN_FLIGHT_SPEAKERS_KEY);
    }
  }

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
  // Two sources show speaker labels.
  let labeled = $state(false);
  let now = $state(Date.now());
  let askSpeakerCount = $state(false);
  let styles = $state<PromptList | null>(null);
  let modelNames = $state<Record<string, string>>({});

  // Re-read when shown, since Settings may have changed styles or the model.
  $effect(() => {
    if (!active) return;
    let current = true;
    getPromptList().then((l) => { if (current) styles = l; }).catch(() => {});
    listSummaryModels()
      .then((ms) => { if (current) modelNames = Object.fromEntries(ms.map((m) => [m.spec.id, m.spec.displayName])); })
      .catch(() => {});
    return () => { current = false; };
  });

  async function chooseStyle(id: string) {
    try {
      styles = await setActivePrompt(id);
    } catch (e) {
      error = String(e);
      try { styles = await getPromptList(); } catch {}
    }
  }
  let stopButton = $state<HTMLButtonElement | undefined>();
  let speakerDialog = $state<HTMLDivElement | undefined>();
  let returnFocus = $state<HTMLElement | null>(null);

  $effect(() => {
    if (askSpeakerCount) speakerDialog?.querySelector('button')?.focus();
    else returnFocus?.focus();
  });

  function confineDialogFocus(event: KeyboardEvent) {
    if (event.key !== 'Tab' || !speakerDialog) return;
    const buttons = Array.from(speakerDialog.querySelectorAll<HTMLButtonElement>('button:not(:disabled)'));
    const first = buttons[0];
    const last = buttons.at(-1);
    if (!first || !last) return;
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  }

  let recording = $derived(status === 'recording');
  let working = $derived(isWorking(status));
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
      if (result || finished.includes(p.state) || p.state === 'awaitingModel') {
        // Keep the id while awaitingModel, so a reload still finds the session.
        if (finished.includes(p.state)) remember(null);
        getSession(p.sessionId).then((s) => (result = s));
      }
    });
    // The window may reload while a session runs.
    currentRecording().then(async (recording) => {
      const id = recording ?? sessionStorage.getItem(IN_FLIGHT_KEY);
      if (!id) return;
      sessionId = id;
      if (recording) status = 'recording';
      const session = await getSession(id);
      // Start may have begun a new session while this fetch ran.
      if (sessionId !== id) return;
      if (!recording && (!session || finished.includes(session.status))) {
        // It ended while the window reloaded.
        sessionId = null;
        remember(null);
        result = session;
        return;
      }
      if (session && status === null) status = session.status;
      if (session?.status === 'awaitingModel') result = session;
      labeled = session?.inputMode === 'micAndSystem';
      if (recording) sessionStorage.setItem(IN_FLIGHT_SPEAKERS_KEY, sessionStorage.getItem(IN_FLIGHT_SPEAKERS_KEY) ?? 'false');
      startedAt = session?.createdAt ?? Date.now();
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  });

  async function start() {
    busy = true;
    error = denied = null;
    result = progress = null;
    let attempt: string | null = null;
    try {
      const settings = await getRecordingSettings();
      const app = settings.inputMode === 'mic' ? null : settings.targetApp;
      const title = `Session ${new Date().toLocaleString()}`;
      attempt = crypto.randomUUID();
      sessionStorage.setItem(IN_FLIGHT_START_KEY, attempt);
      sessionStorage.setItem(IN_FLIGHT_SPEAKERS_KEY, String(settings.multiSpeaker));
      const session = await startRecording(settings.inputMode, app, title);
      if (sessionStorage.getItem(IN_FLIGHT_START_KEY) === attempt) sessionStorage.removeItem(IN_FLIGHT_START_KEY);
      sessionId = session.id;
      remember(session.id);
      labeled = session.inputMode === 'micAndSystem';
      status = session.status;
      startedAt = Date.now();
    } catch (e) {
      if (attempt && sessionStorage.getItem(IN_FLIGHT_START_KEY) === attempt) {
        sessionStorage.removeItem(IN_FLIGHT_START_KEY);
        sessionStorage.removeItem(IN_FLIGHT_SPEAKERS_KEY);
      }
      // Refused by the licence check: the gate explains it.
      const refused = accessDenied(e);
      if (refused) accessState.set(refused);
      else {
        denied = deniedPrivacy(String(e));
        error = String(e);
      }
    } finally {
      busy = false;
    }
  }

  async function stop() {
    try {
      if (sessionStorage.getItem(IN_FLIGHT_SPEAKERS_KEY) === 'true') {
        returnFocus = stopButton ?? (document.activeElement instanceof HTMLElement ? document.activeElement : null);
        askSpeakerCount = true;
        await tick();
        speakerDialog?.querySelector('button')?.focus();
        return;
      }
    } catch (e) {
      error = String(e);
      return;
    }
    await stopWithCount(null);
  }

  async function stopWithCount(numSpeakers: number | null) {
    askSpeakerCount = false;
    busy = true;
    try {
      await stopRecording(numSpeakers);
      // Its "Transcribing…" event may already have moved it on.
      if (status === 'recording') status = 'finalizing';
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function cancel() {
    if (!sessionId) return;
    try {
      await cancelJob(sessionId);
      const session = await getSession(sessionId);
      if (session) {
        status = session.status;
        result = session;
        if (finished.includes(session.status)) remember(null);
      }
    } catch (e) { error = String(e); }
  }
</script>

<section>
  <h2>Record</h2>
  <div inert={askSpeakerCount}>
  {#if recording}
    <p>
      {progress?.message ?? 'Recording…'}
      {Math.floor(elapsed / 60)}:{String(elapsed % 60).padStart(2, '0')}
    </p>
    <button bind:this={stopButton} onclick={stop} disabled={busy || askSpeakerCount}>Stop</button>
  {:else if working && status !== 'awaitingModel'}
    <p>
      {progress?.message ?? 'Working…'}
      {#if progress && progress.stepCount > 1}({progress.stepIndex}/{progress.stepCount}){/if}
    </p>
    <button onclick={cancel}>Cancel</button>
  {:else}
    {#if status === 'awaitingModel'}
      <p>{progress?.message ?? 'Waiting for the summary model download…'}</p>
      <button onclick={cancel}>Cancel</button>
    {/if}
    <LicenseGate access={$accessState} onEnter={onEnterLicense} />
    {#if styles}
      <StylePicker prompts={styles.prompts} selectedId={styles.activeId} onSelect={chooseStyle} />
    {/if}
    <button onclick={start} disabled={busy || !speechReady || !allows($accessState)}>
      Start recording
    </button>
  {/if}

  </div>
  {#if askSpeakerCount}
    <div bind:this={speakerDialog} onkeydown={confineDialogFocus} role="dialog" aria-modal="true" aria-labelledby="speaker-count-title" tabindex="-1">
      <h3 id="speaker-count-title">How many people spoke?</h3>
      <button onclick={() => stopWithCount(2)}>2 people</button>
      <button onclick={() => stopWithCount(3)}>3 people</button>
      <button onclick={() => stopWithCount(null)}>4+ (beta)</button>
      <button onclick={() => (askSpeakerCount = false)}>Keep recording</button>
    </div>
  {/if}

  {#if sessionId && (recording || working || status === 'failed')}
    <LiveTranscript {sessionId} {labeled} />
  {/if}

  {#if denied}
    <PermissionNotice pane={denied} />
  {:else if error}
    <p role="alert">{error}</p>
  {/if}

  {#if result}
    {@const session = result}
    <SummaryPanel {session} {active} byline={[
      styles?.prompts.find((p) => p.id === session.promptId)?.name,
      session.modelId && (modelNames[session.modelId] ?? session.modelId),
    ].filter(Boolean).join(' · ')} onUpdate={(updated) => { if (result?.id === updated.id) result = updated; }} />
    {#if session.timings}<Timings timings={session.timings} />{/if}
  {/if}
</section>
