<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { appVersion, checkForUpdates, currentRecording, lockedState, onHeartbeat, onShellMenu, openExternalLink, openSettings, restoreSampleSessions, setShellMenuState, onSessionProgress, onQuitRequested, quitApp, onTrayStartRecording, onTrayStopFailed, onWindowCloseRequested, takeEncryptionNotice, getAppSettings, onAppSettingsChanged, completeOnboarding, getOnboardingState, listSessions } from './lib/api';
  import RecordView from './lib/components/RecordView.svelte';
  import FirstRun from './lib/components/FirstRun.svelte';
  import ModelStatus from './lib/components/ModelStatus.svelte';
  import LockedScreen from './lib/components/LockedScreen.svelte';
  import NoteStyles from './lib/components/NoteStyles.svelte';
  import UpdateAlert from './lib/components/UpdateAlert.svelte';
  import SessionHistory from './lib/components/SessionHistory.svelte';
  import BackgroundAnimation from './lib/components/BackgroundAnimation.svelte';
  import QuitConfirm from './lib/components/QuitConfirm.svelte';
  import { appSettings } from './lib/app-settings';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { navigation, navigationDisabled, navigationLockedBy, type AppView } from './lib/navigation';
  import { modelsStatus, onModelsProgress } from './lib/api';
  import OnboardingTour from './lib/components/OnboardingTour.svelte';
  import type { AppVersion } from './lib/types/AppVersion';
  import type { LockReason } from './lib/types/LockReason';
  import { accessState, allows } from './lib/license';

  // Asked before anything that needs the store mounts.
  let gate = $state<'loading' | 'open' | { locked: LockReason }>('loading');
  let notice = $state(false);

  let version = $state<AppVersion | null>(null);
  let error = $state<string | null>(null);
  let beats = $state(0);
  let interrupted = $state(0);
  let modelProgress = $state<import('./lib/types/ModelProgress').ModelProgress[]>([]);
  let navigationLocked = $derived(navigationLockedBy(modelProgress));
  let view = $derived($navigation.view);
  $effect(() => navigation.update((n) => ({ ...n, navigationLocked })));
  $effect(() => { if (typeof gate === 'object') void setShellMenuState({ selected: false, hasNote: false, recording: false, recoverable: false, locked: true }).catch(() => {}); });
  let speechReady = $state(false);
  let recording = $state(false);
  let recordingKnown = $state(false);
  let quitIntent = $state<'quit' | 'restart' | null>(null);
  let tourOpen = $state(false);
  let tourRun = $state(0);
  let noteStyles = $state<{ requestLeave: (action: () => void, opener?: HTMLElement) => void; newBlank: () => void } | undefined>();
  let history = $state<{ focusSearch: () => void; confirmDeleteAll: () => void; confirmDeleteSelected: () => void; copySelectedNote: () => Promise<void>; recoverFirst: () => void; refreshSessions: () => void } | undefined>();
  let recordView = $state<{ copyResultNote: () => Promise<void>; refreshAfterDeletion: () => Promise<void> } | undefined>();
  let recordHasNote = $state(false);

  function navigate(next: AppView, opener?: HTMLElement, after?: () => void) {
    if (navigationDisabled(navigationLocked)) return;
    const action = () => { navigation.update((n) => ({ ...n, view: next })); after?.(); };
    if (view === 'styles' && next !== 'styles' && noteStyles) noteStyles.requestLeave(action, opener);
    else action();
  }

  function openLicense() {
    void openSettings('License').catch((e) => (error = String(e)));
  }

  async function refreshInterrupted() {
    interrupted = (await listSessions()).filter((s) => s.interrupted && s.hasAudio && s.status === 'failed').length;
  }

  function sessionsChanged() {
    void refreshInterrupted().catch(() => {});
    void recordView?.refreshAfterDeletion().catch((e) => (error = String(e)));
  }

  onMount(() => {
    const showRecord = () => navigate('record');
    window.addEventListener('tray-start-failed', showRecord);
    const find = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'f') {
        event.preventDefault(); history?.focusSearch();
      }
    };
    window.addEventListener('keydown', find);
    const menuRequest = onShellMenu((id) => {
      switch (id) {
        case 'shell-find': history?.focusSearch(); break;
        case 'shell-manage-styles': navigate('styles'); break;
        case 'shell-blank-style': if (!recording && allows($accessState)) navigate('styles', undefined, () => { void tick().then(() => noteStyles?.newBlank()); }); break;
        case 'shell-new-recording': if (!recording && allows($accessState)) navigate('record'); break;
        case 'shell-delete-all': if (!recording) void history?.confirmDeleteAll(); break;
        case 'shell-delete-session': if (!recording) navigate('history', undefined, () => history?.confirmDeleteSelected()); break;
        case 'shell-copy-note': void (view === 'record' ? recordView?.copyResultNote() : history?.copySelectedNote()); break;
        case 'shell-recover': if (!recording && allows($accessState)) navigate('history', undefined, () => history?.recoverFirst()); break;
        case 'shell-update': void checkForUpdates().catch((e) => (error = String(e))); break;
        case 'shell-welcome': showTour(); break;
        case 'shell-restore-sample': if (allows($accessState)) void restoreSampleSessions().then(() => history?.refreshSessions()).catch((e) => (error = String(e))); break;
        case 'shell-support': void openExternalLink('mailto:support@scribular.com').catch((e) => (error = String(e))); break;
        case 'shell-privacy': void openExternalLink('https://www.scribular.com/privacy').catch((e) => (error = String(e))); break;
        case 'shell-site': void openExternalLink('https://www.scribular.com').catch((e) => (error = String(e))); break;
      }
    });
    getCurrentWindow().show().catch(() => {});
    const modelEventsSeen = new Set<string>();
    const modelEvents = onModelsProgress((p) => {
      modelEventsSeen.add(p.modelId);
      modelProgress = [...modelProgress.filter((m) => m.modelId !== p.modelId), p];
    });
    modelEvents.then(() => modelsStatus()).then((snapshot) => {
      modelProgress = [...snapshot.filter((m) => !modelEventsSeen.has(m.modelId)), ...modelProgress.filter((m) => modelEventsSeen.has(m.modelId))];
    }).catch(() => {});
    const quitRequest = onQuitRequested((restart) => {
      if (typeof gate === 'object') void quitApp();
      else quitIntent = restart ? 'restart' : 'quit';
    });
    const trayStart = onTrayStartRecording(() => window.dispatchEvent(new Event('tray-start-recording')));
    const closeRequest = onWindowCloseRequested(() => {
      const hide = () => { void getCurrentWindow().hide().catch((e) => (error = String(e))); };
      if (view === 'styles' && noteStyles) noteStyles.requestLeave(hide);
      else hide();
    });
    const trayStopFailure = onTrayStopFailed((message) => {
      error = message;
      void getCurrentWindow().show().catch((e) => (error = `${message} (${String(e)})`));
    });
    let settingsEventSeen = false;
    const settingsChanged = onAppSettingsChanged((s) => { settingsEventSeen = true; appSettings.set(s); });
    settingsChanged.then(() => getAppSettings()).then((s) => { if (!settingsEventSeen) appSettings.set(s); }).catch((e) => (error = String(e)));
    let recordingSessionId: string | null = null;
    let recordingEvents = 0;
    let latestProgress: import('./lib/types/SessionProgress').SessionProgress | null = null;
    const progress = onSessionProgress((p) => {
      refreshInterrupted().catch(() => {});
      recordingEvents++;
      latestProgress = p;
      if (p.state === 'recording') {
        recordingSessionId = p.sessionId;
        recording = true;
      } else if (p.sessionId === recordingSessionId) recording = false;
    });
    progress.then(() => {
      const eventsAtSnapshot = recordingEvents;
      return currentRecording().then((id) => {
        if (recordingEvents > eventsAtSnapshot && latestProgress?.state === 'recording') id = latestProgress.sessionId;
        recordingSessionId = id;
        recording = id !== null && (latestProgress?.sessionId !== id || latestProgress.state === 'recording');
      });
    }).catch(() => {}).finally(() => { recordingKnown = true; });
    lockedState()
      .then((reason) => {
        gate = reason ? { locked: reason } : 'open';
        if (!reason) {
          takeEncryptionNotice()
            .then((n) => (notice = n))
            .catch((e) => (error = String(e)));
          getOnboardingState()
            .then((state) => { if (tourRun === 0) tourOpen = !state.completed; })
            .catch((e) => { error = String(e); if (tourRun === 0) tourOpen = true; });
        }
      })
      .catch((e) => (error = String(e)));
    appVersion()
      .then((v) => (version = v))
      .catch((e) => (error = String(e)));
    refreshInterrupted().catch(() => {});
    const unlisten = onHeartbeat((beat) => (beats = beat.seq));
    return () => {
      window.removeEventListener('tray-start-failed', showRecord);
      window.removeEventListener('keydown', find);
      menuRequest.then((fn) => fn());
      unlisten.then((fn) => fn());
      progress.then((fn) => fn());
      modelEvents.then((fn) => fn());
      quitRequest.then((fn) => fn());
      trayStart.then((fn) => fn());
      closeRequest.then((fn) => fn());
      trayStopFailure.then((fn) => fn());
      settingsChanged.then((fn) => fn());
    };
  });

  async function finishTour() {
    try {
      await completeOnboarding();
      tourOpen = false;
    } catch (e) {
      error = String(e);
    }
  }

  function showTour() {
    navigate('record');
    tourRun++;
    tourOpen = true;
  }
</script>

{#if typeof gate === 'object'}
  <LockedScreen reason={gate.locked} />
{:else if gate === 'open'}
<main>
  <BackgroundAnimation enabled={$appSettings.showBackgroundAnimation} {recording} />
  <header data-tauri-drag-region><span data-tauri-drag-region>Scribular</span></header>
  {#if quitIntent}<QuitConfirm restart={quitIntent === 'restart'} close={() => (quitIntent = null)} />{/if}
  <UpdateAlert recording={recording || !recordingKnown} />
  {#if notice}
    <p role="status">
      Your earlier test sessions were encrypted. Turn on FileVault to protect copies of the old file
      that macOS may still hold.
      <button onclick={() => (notice = false)}>Dismiss</button>
    </p>
  {/if}
  {#if interrupted}
    <p role="status">{interrupted} interrupted recording{interrupted === 1 ? '' : 's'} — recover?
      <button onclick={() => navigate('history')}>Recover</button>
    </p>
  {/if}
  <FirstRun bind:speechReady />
  <ModelStatus />
  {#if version}
    <h1>{version.name} <span data-testid="version">{version.version}</span></h1>
  {:else if error}
    <p role="alert">{error}</p>
  {:else}
    <p>Loading…</p>
  {/if}
  <SessionHistory bind:this={history} {view} {navigationLocked} {recording} {recordHasNote} recoverable={interrupted > 0} onNavigate={navigate} onLicense={openLicense} onTour={showTour} onSessionsChanged={sessionsChanged}>
    <!-- Keep the recorder mounted so its progress survives navigation. -->
    <div hidden={view !== 'record'}><RecordView bind:this={recordView}
        {speechReady}
        active={view === 'record'}
        onEnterLicense={openLicense}
        onResultNote={(hasNote) => (recordHasNote = hasNote)}
      />
    </div>
    {#if view === 'styles'}<NoteStyles bind:this={noteStyles} />{/if}
  </SessionHistory>
  {#if tourOpen}{#key tourRun}<OnboardingTour onComplete={finishTour} />{/key}{/if}
  <p>Heartbeat: {beats}</p>
  {#if error && version}<p role="alert">{error}</p>{/if}
</main>
{:else if error}
  <p role="alert">{error}</p>
{/if}
<style>
  header { height: 30px; line-height: 30px; padding-left: 70px; user-select: none; }
</style>
