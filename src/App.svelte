<script lang="ts">
  import { onMount } from 'svelte';
  import { appVersion, completeOnboarding, getOnboardingState, listSessions, lockedState, onHeartbeat, onSessionProgress, takeEncryptionNotice } from './lib/api';
  import RecordView from './lib/components/RecordView.svelte';
  import Settings from './lib/components/Settings.svelte';
  import FirstRun from './lib/components/FirstRun.svelte';
  import ModelStatus from './lib/components/ModelStatus.svelte';
  import LockedScreen from './lib/components/LockedScreen.svelte';
  import TrialBadge from './lib/components/TrialBadge.svelte';
  import UpdateBar from './lib/components/UpdateBar.svelte';
  import SessionHistory from './lib/components/SessionHistory.svelte';
  import OnboardingTour from './lib/components/OnboardingTour.svelte';
  import { followAccessState } from './lib/license';
  import type { AppVersion } from './lib/types/AppVersion';
  import type { LockReason } from './lib/types/LockReason';

  // Asked before anything that needs the store mounts.
  let gate = $state<'loading' | 'open' | { locked: LockReason }>('loading');
  let notice = $state(false);

  let version = $state<AppVersion | null>(null);
  let error = $state<string | null>(null);
  let beats = $state(0);
  let interrupted = $state(0);
  let view = $state<'record' | 'history' | 'settings'>('record');
  let speechReady = $state(false);
  let tourOpen = $state(false);
  let tourRun = $state(0);
  let settings = $state<{ requestLeave: (action: () => void, opener?: HTMLElement) => void } | undefined>();

  function navigate(next: typeof view, opener?: HTMLElement) {
    const action = () => (view = next);
    if (view === 'settings' && next !== 'settings' && settings) settings.requestLeave(action, opener);
    else action();
  }

  async function refreshInterrupted() {
    interrupted = (await listSessions()).filter((s) => s.interrupted && s.hasAudio && s.status === 'failed').length;
  }

  onMount(() => {
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
    const progress = onSessionProgress(() => { refreshInterrupted().catch(() => {}); });
    const unlisten = onHeartbeat((beat) => (beats = beat.seq));
    const stopLicense = followAccessState();
    return () => {
      unlisten.then((fn) => fn());
      progress.then((fn) => fn());
      stopLicense();
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

  function showTour(event: MouseEvent) {
    navigate('record', event.currentTarget instanceof HTMLElement ? event.currentTarget : undefined);
    tourRun++;
    tourOpen = true;
  }
</script>

{#if typeof gate === 'object'}
  <LockedScreen reason={gate.locked} />
{:else if gate === 'open'}
<main>
  <UpdateBar />
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
  <nav>
    <button onclick={(event) => navigate('record', event.currentTarget)} aria-pressed={view === 'record'}>Record</button>
    <button onclick={(event) => navigate('history', event.currentTarget)} aria-pressed={view === 'history'}>History</button>
    <button data-tour="settings" onclick={(event) => navigate('settings', event.currentTarget)} aria-pressed={view === 'settings'}>Settings</button>
    <button onclick={showTour}>Show tour</button>
    <TrialBadge onclick={() => navigate('settings')} />
  </nav>
  <!-- Hidden, not unmounted, so a session in flight keeps its progress. -->
  <div hidden={view !== 'record'}><RecordView
      {speechReady}
      active={view === 'record'}
      onEnterLicense={() => navigate('settings')}
    />
  </div>
  <div hidden={view !== 'history'}><SessionHistory active={view === 'history'} onSessionsChanged={() => refreshInterrupted().catch(() => {})} /></div>
  {#if view === 'settings'}
    <Settings bind:this={settings} />
  {/if}
  {#if tourOpen}{#key tourRun}<OnboardingTour onNavigate={navigate} onComplete={finishTour} />{/key}{/if}
  <p>Heartbeat: {beats}</p>
</main>
{:else if error}
  <p role="alert">{error}</p>
{/if}
