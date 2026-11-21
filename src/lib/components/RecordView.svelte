<script lang="ts">
  // Records with the source chosen here and the remaining saved preferences.
  import { onMount, tick } from 'svelte';
  import {
    accessDenied,
    cancelJob,
    currentRecording,
    deniedPrivacy,
    getPromptList,
    getRecordingSettings,
    getRecordingSource,
    setRecordingSource,
    listAudioApps,
    getOnboardingState,
    markAppAudioExplainerShown,
    systemAudioPermissionStatus,
    openPrivacySettings,
    recordingState,
    onRecordingState,
    onSessionLevels,
    pauseRecording,
    resumeRecording,
    getSession,
    listSummaryModels,
    onSessionProgress,
    onSessionTranscriptWaiting,
    setActivePrompt,
    startRecording,
    stopRecording,
    notifyTrayStartFailure,
  } from '../api';
  import type { ModelProgress } from '../types/ModelProgress';
  import type { Privacy } from '../types/Privacy';
  import type { PromptList } from '../types/PromptList';
  import type { Session } from '../types/Session';
  import type { SessionProgress } from '../types/SessionProgress';
  import type { SessionStatus } from '../types/SessionStatus';
  import type { RecordingSource } from '../types/RecordingSource';
  import type { RecordingState } from '../types/RecordingState';
  import type { SystemAudioPermissionStatus } from '../types/SystemAudioPermissionStatus';
  import type { SessionLevel } from '../types/SessionLevel';
  import LiveTranscript from './LiveTranscript.svelte';
  import Timings from './Timings.svelte';
  import SummaryPanel from './SummaryPanel.svelte';
  import StylePicker from './StylePicker.svelte';
  import PermissionNotice from './PermissionNotice.svelte';
  import { isWorking } from './record-status';
  import LicenseGate from './LicenseGate.svelte';
  import { accessState, allows } from '../license';
  import { getCoachContext } from '../coach-context';

  const coach = getCoachContext();

  let {
    speech,
    modelError = false,
    preparingModels = false,
    onRetryModels = () => {},
    active = true,
    onEnterLicense = () => {},
    onResultNote = () => {},
    coachInterrupted = false,
    onStartFailed = () => {},
    onDismissCoachInterruption = () => {},
  }: { speech?: ModelProgress; modelError?: boolean; preparingModels?: boolean; onRetryModels?: () => void; active?: boolean; onEnterLicense?: () => void; onResultNote?: (hasNote: boolean) => void; coachInterrupted?: boolean; onDismissCoachInterruption?: () => void; onStartFailed?: () => void } = $props();

  let speechReady = $derived(speech?.state === 'ready');
  let speechPercent = $derived(speech && speech.totalBytes > 0
    ? Math.min(100, Math.floor(speech.downloadedBytes / speech.totalBytes * 100))
    : 0);
  const recordViewId = $props.id();
  const modelStatusId = `${recordViewId}-speech-model`;

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
  let transcriptWaiting = $state<{ sessionId: string; waiting: boolean } | null>(null);
  // The finished session's result.
  let result = $state<Session | null>(null);
  let summaryPanel = $state<{ copy: () => Promise<void> } | undefined>();
  $effect(() => onResultNote(!!result?.summary));
  export async function copyResultNote() { if (active && result?.summary) await summaryPanel?.copy(); }
  export function toggleRecording() {
    if (busy || askSpeakerCount) return;
    if (recording) void stop();
    else if (denied !== 'microphone' && (source.location !== 'onCall' || source.targetApp) && allows($accessState)) void start();
  }
  export function toggleRecordingPause() {
    if (recording && !busy && !askSpeakerCount) void togglePause();
  }
  export function cancelDialog() {
    if (askSpeakerCount) { askSpeakerCount = false; return true; }
    if (pickerOpen) { pickerOpen = false; return true; }
    return false;
  }
  export async function refreshAfterDeletion() {
    const id = sessionId ?? result?.id;
    if (!id || await getSession(id)) return;
    if (sessionId === id) {
      resultFetch++;
      sessionId = null;
      status = null;
      progress = null;
      transcriptWaiting = null;
      remember(null);
    }
    if (result?.id === id) result = null;
  }
  let resultFetch = 0;
  let busy = $state(false);
  let error = $state<string | null>(null);
  let denied = $state<Privacy | null>(null);
  let startedAt = $state(0);
  let trayStart = false;
  // Two sources show speaker labels.
  let labeled = $state(false);
  let now = $state(Date.now());
  let source = $state<RecordingSource>({ location: 'inRoom', targetApp: null });
  let sourceReady = $state(false);
  let pickerOpen = $state(false);
  let firstCall = $state(false);
  let explainerShown = $state(false);
  let onboardingReady = $state(false);
  let explainerError = $state<string | null>(null);
  let permissionStatus = $state<SystemAudioPermissionStatus>('unknown');
  let statusError = $state<string | null>(null);
  let apps = $state<string[]>([]);
  let level = $state<Partial<Record<'mic' | 'system', SessionLevel>>>({});
  let capture = $state<RecordingState | null>(null);
  let finalElapsedMs = $state<number | null>(null);
  const commonApps = ['Zoom', 'Microsoft Teams', 'FaceTime', 'Slack', 'Discord', 'Google Chrome', 'Safari', 'Firefox'];
  let askSpeakerCount = $state(false);
  let styles = $state<PromptList | null>(null);
  let modelNames = $state<Record<string, string>>({});

  // Re-read when shown, since Settings may have changed styles or the model.
  $effect(() => {
    if (!active) return;
    let current = true;
    const currentStyles = coach?.styles();
    if (currentStyles) styles = currentStyles;
    else getPromptList().then((l) => { if (current) styles = l; }).catch(() => {});
    listSummaryModels()
      .then((ms) => { if (current) modelNames = Object.fromEntries(ms.map((m) => [m.spec.id, m.spec.displayName])); })
      .catch(() => {});
    return () => { current = false; };
  });

  async function chooseStyle(id: string) {
    try {
      styles = await setActivePrompt(id);
      coach?.updateStyles(styles);
    } catch (e) {
      error = String(e);
      try { styles = await getPromptList(); } catch {}
    }
  }
  let stopButton = $state<HTMLButtonElement | undefined>();
  let speakerDialog = $state<HTMLDivElement | undefined>();
  let callDialog = $state<HTMLDialogElement | undefined>();
  let returnFocus = $state<HTMLElement | null>(null);

  $effect(() => {
    if (askSpeakerCount) speakerDialog?.querySelector('button')?.focus();
    else returnFocus?.focus();
  });

  $effect(() => {
    if (!firstCall || !callDialog) return;
    callDialog.showModal();
    return () => callDialog?.close();
  });

  function confineDialogFocus(event: KeyboardEvent) {
    if (event.key === 'Escape') { askSpeakerCount = false; return; }
    if (event.key === 'Enter' && event.target === speakerDialog) { void stopWithCount(2); return; }
    if (event.key !== 'Tab' || !speakerDialog) return;
    const buttons = speakerDialog.querySelectorAll('button');
    if (event.shiftKey && (document.activeElement === buttons[0] || document.activeElement === speakerDialog)) {
      event.preventDefault();
      buttons[buttons.length - 1]?.focus();
    } else if (!event.shiftKey && (document.activeElement === buttons[buttons.length - 1] || document.activeElement === speakerDialog)) {
      event.preventDefault();
      buttons[0]?.focus();
    }
  }

  let recording = $derived(status === 'recording');
  let appAudioDenied = $derived(capture?.sessionId === sessionId && capture?.appAudioStatus === 'denied');
  let callPermissionStatus = $derived(capture?.sessionId === sessionId && capture?.appAudioStatus === 'capturing' ? 'granted' : appAudioDenied ? 'denied' : permissionStatus);
  let working = $derived(isWorking(status));
  let elapsedMs = $derived(capture?.sessionId === sessionId
    ? (capture.status === 'paused' ? capture.activeElapsedMs : (level.mic?.activeElapsedMs ?? capture.activeElapsedMs))
    : (finalElapsedMs ?? Math.max(0, now - startedAt)));
  let elapsed = $derived(Math.floor(elapsedMs / 1000));

  // The clock only ticks during a recording.
  $effect(() => {
    if (!recording) return;
    now = Date.now();
    const tick = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(tick);
  });

  onMount(() => {
    getRecordingSource().then((saved) => { source = saved; sourceReady = true; }).catch((e) => (error = String(e)));
    void loadOnboarding();
    const startFromTray = () => { if (trayStart || busy) return; trayStart = true; void start(); };
    window.addEventListener('tray-start-recording', startFromTray);
    const unlisten = onSessionProgress((p) => {
      if (p.sessionId !== sessionId) {
        if (p.state !== 'recording' || busy) return;
        sessionId = p.sessionId;
        remember(sessionId);
        startedAt = Date.now();
        result = null;
        const fetch = ++resultFetch;
        void getSession(p.sessionId).then((session) => {
          if (session && fetch === resultFetch && p.sessionId === sessionId) {
            labeled = session.inputMode === 'micAndSystem';
            startedAt = session.createdAt;
          }
        }).catch(() => {});
      }
      if (p.state === 'recording') firstCall = false;
      progress = p;
      status = p.state;
      if (result || finished.includes(p.state) || p.state === 'awaitingModel') {
        const fetch = ++resultFetch;
        // Keep the id while awaitingModel, so a reload still finds the session.
        if (finished.includes(p.state)) remember(null);
        getSession(p.sessionId).then((s) => {
          if (sessionId === p.sessionId && resultFetch === fetch) result = s;
        });
      }
    });
    const unlistenTranscriptWaiting = onSessionTranscriptWaiting((state) => {
      if (state.waiting) transcriptWaiting = state;
      else if (transcriptWaiting?.sessionId === state.sessionId) transcriptWaiting = null;
    });
    const unlistenState = onRecordingState((state) => {
      capture = state;
      if (state?.appAudioStatus === 'capturing' && denied === 'systemAudio') denied = null;
      if (state?.sessionId === sessionId) status = 'recording';
    });
    const unlistenLevels = onSessionLevels((next) => {
      if (next.sessionId === sessionId) level[next.source] = next;
    });
    recordingState().then((state) => { if (state) capture = state; }).catch(() => {});
    // The window may reload while a session runs.
    currentRecording().then(async (recording) => {
      const id = recording ?? sessionStorage.getItem(IN_FLIGHT_KEY);
      if (!id) return;
      const fetch = ++resultFetch;
      sessionId = id;
      if (recording) status = 'recording';
      const session = await getSession(id);
      // Start may have begun a new session while this fetch ran.
      if (sessionId !== id || resultFetch !== fetch) return;
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
      window.removeEventListener('tray-start-recording', startFromTray);
      unlisten.then((fn) => fn());
      unlistenTranscriptWaiting.then((fn) => fn());
      unlistenState.then((fn) => fn());
      unlistenLevels.then((fn) => fn());
    };
  });

  async function loadOnboarding() {
    try {
      explainerShown = (await getOnboardingState()).appAudioExplainerShown;
      onboardingReady = true;
      explainerError = null;
    } catch (e) { explainerError = String(e); }
  }

  async function showCallExplainer() {
    firstCall = true;
    permissionStatus = 'unknown';
    if (!explainerShown) {
      try {
        await markAppAudioExplainerShown();
        explainerShown = true;
        explainerError = null;
      } catch (e) { explainerError = String(e); return; }
    }
    await openCallSettings();
  }

  $effect(() => {
    if (!firstCall || !active) return;
    let current = true;
    const refresh = async () => {
      if (document.visibilityState !== 'visible') return;
      try {
        const status = await systemAudioPermissionStatus();
        if (current) { permissionStatus = status; statusError = null; }
      } catch (e) { if (current) statusError = String(e); }
    };
    void refresh();
    const timer = setInterval(refresh, 2000);
    document.addEventListener('visibilitychange', refresh);
    return () => { current = false; clearInterval(timer); document.removeEventListener('visibilitychange', refresh); };
  });

  async function openCallSettings() {
    try { await openPrivacySettings('systemAudio'); explainerError = null; }
    catch (e) { explainerError = String(e); }
  }

  async function notNow() {
    await chooseLocation('inRoom');
    if (source.location === 'inRoom') { firstCall = pickerOpen = false; denied = error = null; }
  }

  async function start() {
    if (!speechReady) {
      onStartFailed();
      const message = "The speech model isn't downloaded yet.";
      error = message;
      if (trayStart) {
        window.dispatchEvent(new Event('tray-start-failed'));
        try { await notifyTrayStartFailure(message); } catch (e) { error = `${message} (notification failed: ${String(e)})`; }
        trayStart = false;
      }
      return;
    }
    busy = true;
    finalElapsedMs = null;
    resultFetch++;
    error = denied = null;
    result = progress = null;
    let attempt: string | null = null;
    try {
      const settings = await getRecordingSettings();
      const chosenSource = sourceReady ? source : await getRecordingSource();
      const app = chosenSource.location === 'onCall' ? chosenSource.targetApp : null;
      if (chosenSource.location === 'onCall' && !app) throw new Error('Choose an app before recording a call.');
      const title = `Session ${new Date().toLocaleString()}`;
      attempt = crypto.randomUUID();
      sessionStorage.setItem(IN_FLIGHT_START_KEY, attempt);
      sessionStorage.setItem(IN_FLIGHT_SPEAKERS_KEY, String(settings.multiSpeaker));
      const session = await startRecording(chosenSource.location === 'onCall' ? 'micAndSystem' : 'mic', app, title);
      if (sessionStorage.getItem(IN_FLIGHT_START_KEY) === attempt) sessionStorage.removeItem(IN_FLIGHT_START_KEY);
      resultFetch++;
      sessionId = session.id;
      remember(session.id);
      labeled = session.inputMode === 'micAndSystem';
      status = session.status;
      startedAt = Date.now();
      level = {};
      capture = await recordingState().catch(() => null);
      trayStart = false;
    } catch (e) {
      onStartFailed();
      if (attempt && sessionStorage.getItem(IN_FLIGHT_START_KEY) === attempt) {
        sessionStorage.removeItem(IN_FLIGHT_START_KEY);
        sessionStorage.removeItem(IN_FLIGHT_SPEAKERS_KEY);
      }
      const refused = accessDenied(e);
      if (refused) {
        accessState.set(refused);
        if (trayStart) {
          const message = 'Scribular cannot start recording because account access is restricted.';
          window.dispatchEvent(new Event('tray-start-failed'));
          try { await notifyTrayStartFailure(message); }
          catch (noticeError) { error = `${message} (notification failed: ${String(noticeError)})`; }
          trayStart = false;
        }
      }
      else {
        denied = deniedPrivacy(String(e));
        error = String(e);
        if (trayStart) {
          window.dispatchEvent(new Event('tray-start-failed'));
          try {
            await notifyTrayStartFailure(String(e));
          } catch (noticeError) { error = `${String(e)} (notification failed: ${String(noticeError)})`; }
          trayStart = false;
        }
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
    const stoppedElapsedMs = elapsedMs;
    try {
      await stopRecording(numSpeakers);
      finalElapsedMs = stoppedElapsedMs;
      capture = null;
      // Its "Transcribing…" event may already have moved it on.
      if (status === 'recording') status = 'finalizing';
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function chooseLocation(location: RecordingSource['location']) {
    if (location === 'onCall' && !explainerShown) { source = { ...source, location }; await showCallExplainer(); return; }
    if (location === 'onCall' && !source.targetApp) { source = { ...source, location }; await openPicker(); return; }
    const next = { ...source, location };
    try { await setRecordingSource(next); source = next; error = null; }
    catch (e) { error = String(e); }
  }

  async function openPicker() {
    if (busy) return;
    pickerOpen = true;
    try { apps = (await listAudioApps()).map((app) => app.name); denied = error = null; }
    catch (e) { denied = deniedPrivacy(String(e)); error = String(e); }
  }

  async function chooseApp(name: string) {
    if (busy) return;
    const next = { location: 'onCall' as const, targetApp: name };
    try { await setRecordingSource(next); source = next; pickerOpen = false; firstCall = false; error = null; }
    catch (e) { error = String(e); }
  }

  async function togglePause() {
    try {
      if (capture?.status === 'paused') await resumeRecording();
      else await pauseRecording();
      capture = await recordingState();
      error = null;
    } catch (e) { error = String(e); }
  }

  async function cancel() {
    if (!sessionId) return;
    const before = status;
    try {
      await cancelJob(sessionId);
      const session = await getSession(sessionId);
      // A progress event that landed meanwhile is newer than this snapshot.
      if (session && status === before) {
        status = session.status;
        result = session;
        if (finished.includes(session.status)) remember(null);
      }
    } catch (e) { error = String(e); }
  }
</script>

<section inert={askSpeakerCount}>
  <h2>Record</h2>
  {#if coachInterrupted}
    <p role="status">The note style coach closed because a recording started. You can start it again from Note Styles after your recording.
      <button onclick={onDismissCoachInterruption}>Dismiss</button>
    </p>
  {/if}
  {#if !recording}
    <fieldset disabled={!sourceReady || !onboardingReady || busy}>
      <legend>Recording source</legend>
      <label><input type="radio" name="recording-source" checked={source.location === 'inRoom'} onclick={() => chooseLocation('inRoom')} /> In the room</label>
      <label><input type="radio" name="recording-source" checked={source.location === 'onCall'} onclick={() => chooseLocation('onCall')} /> On a call</label>
    </fieldset>
    {#if source.location === 'onCall'}
      <button onclick={openPicker} disabled={busy}>App: {source.targetApp ?? 'Choose an app…'}</button>
      {#if /Chrome|Safari|Firefox|Edge|Brave/i.test(source.targetApp ?? '')}<p>Audio from all browser tabs will be recorded.</p>{/if}
    {/if}
    {#if pickerOpen}
      <div role="dialog" aria-label="Choose a call app" tabindex="-1" onkeydown={(event) => { if (event.key === 'Escape') cancelDialog(); }}>
        <p>Common apps and browsers</p>
        {#each commonApps as name}<button disabled={busy} onclick={() => chooseApp(name)}>{name}</button>{/each}
        <p>Other App — currently playing audio</p>
        {#each apps.filter((name) => !commonApps.includes(name)) as name}<button disabled={busy} onclick={() => chooseApp(name)}>{name}</button>{/each}
        <button disabled={busy} onclick={openPicker}>Refresh</button>
        <button disabled={busy} onclick={() => (pickerOpen = false)}>Close</button>
      </div>
    {/if}
  {/if}
  <div>
  {#if recording}
    <p>
      {progress?.message ?? 'Recording…'}
      {Math.floor(elapsed / 60)}:{String(elapsed % 60).padStart(2, '0')}
    </p>
    <button onclick={togglePause} disabled={busy}>{capture?.status === 'paused' ? 'Resume' : 'Pause'}</button>
    <button bind:this={stopButton} onclick={stop} disabled={busy || askSpeakerCount}>Stop</button>
    <div aria-label="Microphone level"><label>Microphone <meter min="0" max="1" value={level.mic?.level ?? 0}></meter></label></div>
    {#if labeled}
      <div aria-label="App audio level"><label>App audio <meter min="0" max="1" value={level.system?.level ?? 0}></meter></label></div>
      {#if level.system?.hearing === 'warning'}<p role="status">No app audio heard yet. Your microphone is still recording.</p>{/if}
    {/if}
  {:else}
    {#if working && status !== 'awaitingModel'}
      <p>
        {progress?.message ?? 'Working…'}
        {#if progress && progress.stepCount > 1}({progress.stepIndex}/{progress.stepCount}){/if}
      </p>
      <button onclick={cancel}>Cancel</button>
    {/if}
    {#if status === 'awaitingModel'}
      <p>{progress?.message ?? 'Waiting for the summary model download…'}</p>
      <button onclick={cancel}>Cancel</button>
    {/if}
    <LicenseGate access={$accessState} onEnter={onEnterLicense} />
    <div class="record-control">
      <div class="record-ring" class:downloading={!speechReady} style:--speech-progress={`${speechPercent}%`}>
        <button onclick={start} aria-describedby={speechReady ? undefined : modelStatusId} disabled={busy || !sourceReady || denied === 'microphone' || (source.location === 'onCall' && !source.targetApp) || !speechReady || !allows($accessState)}>
          Start recording
        </button>
      </div>
      {#if !speechReady}
        <p id={modelStatusId} role="status">
          {#if speech?.state === 'failed'}
            Speech model download failed{speech.error ? `: ${speech.error}` : '.'}
          {:else if modelError}
            Could not start or check the speech model download.
          {:else if speech?.state === 'verifying'}
            Verifying the speech model…
          {:else}
            Downloading the speech model{speech?.state === 'downloading' && speech.totalBytes > 0 ? ` (${speechPercent}%)` : '…'}
          {/if}
        </p>
        {#if speech?.state === 'failed' || modelError}
          <button onclick={onRetryModels} disabled={preparingModels}>Retry speech model download</button>
        {/if}
      {/if}
    </div>
  {/if}
  {#if styles}
    <StylePicker prompts={styles.prompts} selectedId={styles.activeId} disabled={!allows($accessState)} onSelect={chooseStyle} />
  {/if}

  </div>
  {#if sessionId && (recording || working || status === 'failed')}
    <LiveTranscript {sessionId} {labeled} activeElapsedMs={elapsedMs} waiting={transcriptWaiting?.sessionId === sessionId && transcriptWaiting.waiting} />
  {/if}

  {#if denied || appAudioDenied}
    <PermissionNotice pane={denied ?? 'systemAudio'} />
    {#if denied === 'systemAudio' || appAudioDenied}<button onclick={showCallExplainer}>Retry app audio</button>{/if}
  {:else if capture?.sessionId === sessionId && capture?.appAudioStatus === 'unavailable'}
    <p role="status">App audio is unavailable. Your microphone is still recording. <button onclick={showCallExplainer}>Retry app audio</button></p>
  {:else if error}
    <p role="alert">{error}</p>
  {/if}

  {#if result}
    {@const session = result}
    <SummaryPanel bind:this={summaryPanel} {session} {active} prompts={styles} byline={[
      styles?.prompts.find((p) => p.id === session.promptId)?.name,
      session.modelId && (modelNames[session.modelId] ?? session.modelId),
    ].filter(Boolean).join(' · ')} onUpdate={(updated) => { if (result?.id === updated.id) result = updated; }} />
    {#if session.timings}<Timings timings={session.timings} />{/if}
  {/if}
</section>
{#if !onboardingReady && explainerError}
  <p role="alert">{explainerError} <button onclick={loadOnboarding}>Retry onboarding state</button></p>
{/if}
{#if firstCall}
  <dialog bind:this={callDialog} aria-label="App audio access" oncancel={(event) => { event.preventDefault(); void notNow(); }}>
    <p>On a call records your microphone and one selected app. To include app audio, allow Scribular in macOS Screen &amp; System Audio Recording.</p>
    <p role="status">App audio access: {callPermissionStatus === 'granted' ? 'Granted' : callPermissionStatus === 'denied' ? 'Denied' : 'Unknown — check System Settings or try recording.'}</p>
    {#if explainerError || statusError}<p role="alert">{explainerError ?? statusError} <button onclick={explainerError ? (explainerShown ? openCallSettings : showCallExplainer) : () => systemAudioPermissionStatus().then((status) => { permissionStatus = status; statusError = null; }).catch((e) => { statusError = String(e); })}>Retry</button></p>{/if}
    <button onclick={openCallSettings}>Open System Settings</button>
    {#if source.targetApp}
      <button disabled={!explainerShown} onclick={async () => { await chooseLocation('onCall'); if (source.location === 'onCall') { firstCall = false; if (!recording && callPermissionStatus !== 'granted') await start(); } }}>
        {callPermissionStatus === 'granted' ? 'Continue with selected app' : 'Try recording'}
      </button>
    {:else}
      <button disabled={!explainerShown} onclick={() => { firstCall = false; void openPicker(); }}>Choose an app to try recording</button>
    {/if}
    <button onclick={notNow}>Not Now</button>
  </dialog>
{/if}
{#if askSpeakerCount}
  <div bind:this={speakerDialog} onkeydown={confineDialogFocus} role="dialog" aria-modal="true" aria-labelledby="speaker-count-title" tabindex="-1">
    <h3 id="speaker-count-title">How many people spoke?</h3>
    <button onclick={() => stopWithCount(2)}>2 people</button>
    <button onclick={() => stopWithCount(3)}>3 people</button>
    <button onclick={() => stopWithCount(null)}>4+ (beta)</button>
    <button onclick={() => (askSpeakerCount = false)}>Keep recording</button>
  </div>
{/if}

<style>
  dialog[aria-label="App audio access"] { width: min(28rem, calc(100% - 2rem)); padding: 1.5rem; border: 1px solid GrayText; border-radius: 1rem; background: Canvas; color: CanvasText; box-shadow: 0 1rem 3rem #0004; }
  dialog[aria-label="App audio access"]::backdrop { background: #0007; }
  .record-control { margin-block: .75rem; }
  .record-ring {
    display: inline-grid;
    place-items: center;
    padding: 4px;
    border-radius: 50%;
  }
  .record-ring.downloading {
    background: conic-gradient(Highlight var(--speech-progress), color-mix(in srgb, CanvasText 15%, Canvas) 0);
  }
  .record-ring button {
    width: 6.75rem;
    height: 6.75rem;
    border-radius: 50%;
  }
  .record-ring.downloading button {
    background: Canvas;
    color: GrayText;
    border: 1px solid transparent;
  }
  .record-control p { margin-block: .5rem; }
</style>
