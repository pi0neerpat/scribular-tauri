<script lang="ts">
  import { onMount, setContext, tick } from 'svelte';
  import { appVersion, checkForUpdates, currentRecording, lockedState, onHeartbeat, onShellMenu, openExternalLink, openSettings, restoreSampleSessions, setShellMenuState, onSessionProgress, onQuitRequested, quitApp, onTrayStartRecording, onTrayStopFailed, onWindowCloseRequested, takeEncryptionNotice, getAppSettings, onAppSettingsChanged, listSessions } from './lib/api';
  import RecordView from './lib/components/RecordView.svelte';
  import LockedScreen from './lib/components/LockedScreen.svelte';
  import NoteStyles from './lib/components/NoteStyles.svelte';
  import UpdateAlert from './lib/components/UpdateAlert.svelte';
  import SessionHistory from './lib/components/SessionHistory.svelte';
  import BackgroundAnimation from './lib/components/BackgroundAnimation.svelte';
  import QuitConfirm from './lib/components/QuitConfirm.svelte';
  import { appSettings } from './lib/app-settings';
  import { startAppearance } from './lib/appearance-store';
  import { navigation, navigationDisabled, navigationLockedBy, type AppView } from './lib/navigation';
  import { downloadModels, modelsStatus, onModelsProgress, setMainWindowVisible } from './lib/api';
  import { mergeSnapshot } from './lib/models';
  import WelcomeSheet from './lib/components/WelcomeSheet.svelte';
  import type { ModelProgress } from './lib/types/ModelProgress';
  import type { AppVersion } from './lib/types/AppVersion';
  import type { LockReason } from './lib/types/LockReason';
  import { accessState, allows } from './lib/license';

  import { COACH_CONTEXT, type CoachContext, type CoachTarget } from './lib/coach-context';
  import { dismissFirstNoteTip, getFirstNoteTipSession, onCoachCancelled, subscribeCoachAvailability } from './lib/api';
  import NoteStyleCoach from './lib/components/NoteStyleCoach.svelte';
  import type { CoachAvailability } from './lib/types/CoachAvailability';
  import type { PromptList } from './lib/types/PromptList';

  onMount(() => {
    let stop: (() => void) | undefined;
    let disposed = false;
    void startAppearance().then((cleanup) => { if (disposed) cleanup(); else stop = cleanup; });
    return () => { disposed = true; stop?.(); };
  });

  // Asked before anything that needs the store mounts.
  let gate = $state<'loading' | 'open' | { locked: LockReason }>('loading');
  let notice = $state(false);

  let version = $state<AppVersion | null>(null);
  let error = $state<string | null>(null);
  let beats = $state(0);
  let interrupted = $state(0);
  let modelProgress = $state<ModelProgress[]>([]);
  let speech = $derived(modelProgress.find((m) => m.modelId === 'whisper-base'));
  let navigationLocked = $derived(navigationLockedBy(modelProgress));
  let modelError = $state(false);
  let preparingModels = $state(false);
  let retryModels = $state<() => void>(() => {});
  let view = $derived($navigation.view);
  $effect(() => navigation.update((n) => ({ ...n, navigationLocked })));
  $effect(() => { if (typeof gate === 'object') void setShellMenuState({ selected: false, hasNote: false, recording: false, recoverable: false, locked: true }).catch(() => {}); });
  let recording = $state(false);
  let recordingKnown = $state(false);
  let quitIntent = $state<'quit' | 'restart' | null>(null);
  let welcomeOpen = $state(false);
  let welcome = $state<{ show: () => void }>();
  let noteStyles = $state<{ requestLeave: (action: () => void, opener?: HTMLElement) => void; newBlank: () => void } | undefined>();
  let history = $state<{ focusSearch: () => Promise<void>; toggleSidebar: () => void; moveSession: (direction: -1 | 1) => void; confirmDeleteAll: () => void; confirmDeleteSelected: () => void; copySelectedNote: () => Promise<void>; recoverFirst: () => void; refreshSessions: () => void } | undefined>();
  let recordView = $state<{ copyResultNote: () => Promise<void>; toggleRecording: () => void; toggleRecordingPause: () => void; cancelDialog: () => boolean; refreshAfterDeletion: () => Promise<void> } | undefined>();
  let recordHasNote = $state(false);

  let coach = $state<CoachTarget | null>(null);
  let coachAvailability = $state<CoachAvailability | null>(null);
  let coachInterrupted = $state(false);
  let pendingCoachInterruption = false;
  let firstNoteTipId = $state<string | null>(null);
  let tipDismissed = false;
  let sharedStyles = $state<PromptList | null>(null);
  let coachOpener: HTMLElement | null = null;
  let coachDraftId: string | null = null;
  let cancelledDraftId: string | null = null;
  let coachOpened: (() => void) | undefined;

  setContext<CoachContext>(COACH_CONTEXT, {
    availability: () => coachAvailability,
    open: openCoach,
    firstNoteTip: (id) => firstNoteTipId === id,
    dismissTip: dismissTip,
    styles: () => sharedStyles,
    updateStyles: (styles) => (sharedStyles = styles),
  });

  async function loadFirstNoteTip() {
    if (tipDismissed || firstNoteTipId) return;
    try {
      const id = await getFirstNoteTipSession();
      if (!tipDismissed) firstNoteTipId = id;
    } catch {}
  }

  function dismissTip() {
    tipDismissed = true;
    firstNoteTipId = null;
    void dismissFirstNoteTip().catch(() => { tipDismissed = false; void loadFirstNoteTip(); });
  }

  function openCoach(target: CoachTarget, opener?: HTMLElement, onOpen?: () => void) {
    if (coach || gate !== 'open' || !coachAvailability || coachAvailability.reason) return;
    const open = () => {
      if (coach || coachAvailability?.reason !== null) return;
      coachOpener = opener ?? (document.activeElement instanceof HTMLElement ? document.activeElement : null);
      coachDraftId = null;
      cancelledDraftId = null;
      coachOpened = onOpen;
      coach = target;
    };
    if (view === 'styles' && noteStyles) noteStyles.requestLeave(open, opener);
    else open();
  }

  function closeCoach(restoreFocus = true) {
    const opener = coachOpener;
    coach = null;
    coachOpener = null;
    coachDraftId = null;
    coachOpened = undefined;
    if (restoreFocus) void tick().then(() => { if (opener?.isConnected && !opener.closest('[hidden]')) opener.focus(); });
  }

  function coachCancelled(id: string, interrupted: boolean) {
    if (!coach || (coachDraftId && id !== coachDraftId)) return;
    cancelledDraftId = id;
    if (coachDraftId === id) closeCoach(!interrupted);
    if (interrupted) {
      if (recording) coachInterrupted = true;
      else pendingCoachInterruption = true;
      navigation.update((state) => ({ ...state, view: 'record' }));
    }
  }

  $effect(() => {
    if (gate !== 'open') return;
    let mounted = true;
    const availability = subscribeCoachAvailability((state) => {
      if (mounted) coachAvailability = state;
    }).catch((e) => { if (mounted) error = String(e); return () => {}; });
    const cancelled = onCoachCancelled(({ draftId, reason }) => {
      if (mounted) coachCancelled(draftId, reason === 'recording');
    });
    return () => {
      mounted = false;
      void availability.then((off) => off()).catch(() => {});
      void cancelled.then((off) => off()).catch(() => {});
    };
  });

  function navigate(next: AppView, opener?: HTMLElement, after?: () => void) {
    if (coach || navigationDisabled(navigationLocked, next, view)) return;
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

  function modalOpen() {
    return !!(quitIntent || [...document.querySelectorAll('dialog[open], [role="dialog"], [role="alertdialog"]')].some((dialog) => !dialog.closest('[hidden]')));
  }

  onMount(() => {
    const showRecord = () => { pendingCoachInterruption = false; navigate('record'); };
    window.addEventListener('tray-start-failed', showRecord);
    const find = (event: KeyboardEvent) => {
      if (event.metaKey && event.key === '.') {
        const dialog = document.querySelector<HTMLDialogElement>('dialog[open]');
        if (dialog) { event.preventDefault(); dialog.dispatchEvent(new Event('cancel', { cancelable: true })); return; }
        const ownerDialog = (event.target as HTMLElement | null)?.closest('[role="dialog"], [role="alertdialog"]');
        if (ownerDialog) { event.preventDefault(); ownerDialog.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true })); return; }
        if (recordView?.cancelDialog()) event.preventDefault();
        return;
      }
      if (!coach && !welcomeOpen && !modalOpen() && (event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'f') {
        event.preventDefault(); void history?.focusSearch();
      }
    };
    window.addEventListener('keydown', find);
    const menuRequest = onShellMenu((id) => {
      if (coach || welcomeOpen || modalOpen()) return;
      switch (id) {
        case 'shell-example-style':
        case 'shell-coach': openCoach({}); break;
        case 'shell-find': void history?.focusSearch(); break;
        case 'shell-record': if (recording) recordView?.toggleRecording(); else navigate('record', undefined, () => recordView?.toggleRecording()); break;
        case 'shell-pause': recordView?.toggleRecordingPause(); break;
        case 'shell-previous': history?.moveSession(-1); break;
        case 'shell-next': history?.moveSession(1); break;
        case 'shell-sidebar': history?.toggleSidebar(); break;
        case 'shell-manage-styles': navigate('styles'); break;
        case 'shell-blank-style': if (!recording && allows($accessState)) navigate('styles', undefined, () => { void tick().then(() => noteStyles?.newBlank()); }); break;
        case 'shell-new-recording': if (!recording && allows($accessState)) navigate('record'); break;
        case 'shell-delete-all': if (!recording) void history?.confirmDeleteAll(); break;
        case 'shell-delete-session': if (!recording) navigate('history', undefined, () => history?.confirmDeleteSelected()); break;
        case 'shell-copy-note': void (view === 'record' ? recordView?.copyResultNote() : history?.copySelectedNote()); break;
        case 'shell-recover': if (!recording && allows($accessState)) navigate('history', undefined, () => history?.recoverFirst()); break;
        case 'shell-update': void checkForUpdates().catch((e) => (error = String(e))); break;
        case 'shell-welcome': welcome?.show(); break;
        case 'shell-restore-sample': if (allows($accessState)) void restoreSampleSessions().then(() => history?.refreshSessions()).catch((e) => (error = String(e))); break;
        case 'shell-support': void openExternalLink('mailto:support@scribular.com').catch((e) => (error = String(e))); break;
        case 'shell-privacy': void openExternalLink('https://www.scribular.com/privacy').catch((e) => (error = String(e))); break;
        case 'shell-site': void openExternalLink('https://www.scribular.com').catch((e) => (error = String(e))); break;
      }
    });
    setMainWindowVisible(true).catch(() => {});
    const quitRequest = onQuitRequested((restart) => {
      if (typeof gate === 'object') void quitApp();
      else quitIntent = restart ? 'restart' : 'quit';
    });
    const trayStart = onTrayStartRecording(() => {
      if (welcomeOpen) void setMainWindowVisible(true).catch((e) => (error = String(e)));
      else window.dispatchEvent(new Event('tray-start-recording'));
    });
    const closeRequest = onWindowCloseRequested(() => {
      if (coach) closeCoach(false);
      const hide = () => { void setMainWindowVisible(false).catch((e) => (error = String(e))); };
      if (view === 'styles' && noteStyles) noteStyles.requestLeave(hide);
      else hide();
    });
    const trayStopFailure = onTrayStopFailed((message) => {
      error = message;
      void setMainWindowVisible(true).catch((e) => (error = `${message} (${String(e)})`));
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
      if (p.state === 'done') void loadFirstNoteTip();
      if (p.state === 'recording') {
        recordingSessionId = p.sessionId;
        recording = true;
        if (pendingCoachInterruption) {
          pendingCoachInterruption = false;
          coachInterrupted = true;
        }
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
          void loadFirstNoteTip();
          takeEncryptionNotice()
            .then((n) => (notice = n))
            .catch((e) => (error = String(e)));
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
      quitRequest.then((fn) => fn());
      trayStart.then((fn) => fn());
      closeRequest.then((fn) => fn());
      trayStopFailure.then((fn) => fn());
      settingsChanged.then((fn) => fn());
    };
  });

  $effect(() => {
    if (gate !== 'open') return;
    let disposed = false;
    let remove: (() => void) | undefined;
    let snapshotLoaded = false;
    const prepare = async () => {
      if (disposed || preparingModels) return;
      preparingModels = true;
      try {
        remove ??= await onModelsProgress((progress) => {
          if (disposed) return;
          modelProgress = [...modelProgress.filter((m) => m.modelId !== progress.modelId), progress];
          if (progress.modelId === 'whisper-base' && (progress.state === 'ready' || progress.state === 'failed')) preparingModels = false;
        });
        if (disposed) { remove(); return; }
        if (!snapshotLoaded) {
          const snapshot = await modelsStatus();
          if (disposed) return;
          modelProgress = mergeSnapshot(snapshot, modelProgress);
          snapshotLoaded = true;
        }
        await downloadModels();
        if (!disposed) modelError = false;
      } catch {
        if (!disposed) { modelError = true; preparingModels = false; }
      }
    };
    retryModels = prepare;
    // Start with a live listener so a delayed snapshot cannot overwrite progress.
    void Promise.resolve().then(prepare);
    return () => { disposed = true; remove?.(); };
  });
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
  {#if version}
    <h1>{version.name} <span data-testid="version">{version.version}</span></h1>
  {:else if error}
    <p role="alert">{error}</p>
  {:else}
    <p>Loading…</p>
  {/if}
  <SessionHistory bind:this={history} {view} {navigationLocked} {recording} {recordHasNote} recoverable={interrupted > 0} onNavigate={navigate} onLicense={openLicense} onSessionsChanged={sessionsChanged}>
    <!-- Keep the recorder mounted so its progress survives navigation. -->
    <div hidden={view !== 'record'}><RecordView bind:this={recordView}
        {speech}
        {modelError}
        {preparingModels}
        onRetryModels={retryModels}
        {coachInterrupted}
        onDismissCoachInterruption={() => (coachInterrupted = false)}
        onStartFailed={() => (pendingCoachInterruption = false)}
        active={view === 'record'}
        onEnterLicense={openLicense}
        onResultNote={(hasNote) => (recordHasNote = hasNote)}
      />
    </div>
    {#if view === 'styles'}<NoteStyles bind:this={noteStyles} />{/if}
  </SessionHistory>
  <WelcomeSheet bind:this={welcome} bind:open={welcomeOpen} suspended={!!quitIntent} />
  <p>Heartbeat: {beats}</p>
  {#if error && version}<p role="alert">{error}</p>{/if}
</main>
{#if coach}
  <NoteStyleCoach target={coach} availability={coachAvailability} onClose={() => closeCoach()}
    onDraft={(id) => {
      if (!coach || cancelledDraftId === id) { closeCoach(false); return false; }
      coachDraftId = id;
      coachOpened?.();
      return true;
    }}
    onSaved={(styles) => { sharedStyles = styles; closeCoach(); }}
  />
{/if}
{:else if error}
  <p role="alert">{error}</p>
{/if}
<style>
  header { height: 30px; line-height: 30px; padding-left: 70px; user-select: none; }
</style>
