<script lang="ts">
  import { onMount } from 'svelte';
  import { appVersion, currentRecording, lockedState, onHeartbeat, onSessionProgress, onQuitRequested, onTrayStartRecording, takeEncryptionNotice, getAppSettings } from './lib/api';
  import RecordView from './lib/components/RecordView.svelte';
  import Settings from './lib/components/Settings.svelte';
  import FirstRun from './lib/components/FirstRun.svelte';
  import ModelStatus from './lib/components/ModelStatus.svelte';
  import LockedScreen from './lib/components/LockedScreen.svelte';
  import TrialBadge from './lib/components/TrialBadge.svelte';
  import UpdateBar from './lib/components/UpdateBar.svelte';
  import BackgroundAnimation from './lib/components/BackgroundAnimation.svelte';
  import QuitConfirm from './lib/components/QuitConfirm.svelte';
  import { appSettings } from './lib/app-settings';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { navigation, navigationDisabled, navigationLockedBy } from './lib/navigation';
  import { modelsStatus, onModelsProgress } from './lib/api';
  import { followAccessState } from './lib/license';
  import type { AppVersion } from './lib/types/AppVersion';
  import type { LockReason } from './lib/types/LockReason';

  // Asked before anything that needs the store mounts.
  let gate = $state<'loading' | 'open' | { locked: LockReason }>('loading');
  let notice = $state(false);

  let version = $state<AppVersion | null>(null);
  let error = $state<string | null>(null);
  let beats = $state(0);
  let modelProgress = $state<import('./lib/types/ModelProgress').ModelProgress[]>([]);
  let navigationLocked = $derived(navigationLockedBy(modelProgress));
  let view = $derived($navigation.view);
  $effect(() => navigation.update((n) => ({ ...n, navigationLocked })));
  let speechReady = $state(false);
  let recording = $state(false);
  let confirmQuitOpen = $state(false);
  let settings = $state<{ requestLeave: (action: () => void, opener?: HTMLElement) => void } | undefined>();

  function navigate(next: typeof view, opener?: HTMLElement) {
    if (navigationDisabled(navigationLocked)) return;
    const action = () => navigation.update((n) => ({ ...n, view: next }));
    if (view === 'settings' && next !== 'settings' && settings) settings.requestLeave(action, opener);
    else action();
  }

  onMount(() => {
    const showRecord = () => navigation.update((n) => ({ ...n, view: 'record' }));
    window.addEventListener('tray-start-failed', showRecord);
    getCurrentWindow().show().catch(() => {});
    modelsStatus().then((m) => (modelProgress = m)).catch(() => {});
    const modelEvents = onModelsProgress((p) => {
      modelProgress = [...modelProgress.filter((m) => m.modelId !== p.modelId), p];
    });
    const quitRequest = onQuitRequested(() => (confirmQuitOpen = true));
    const trayStart = onTrayStartRecording(() => window.dispatchEvent(new Event('tray-start-recording')));
    getAppSettings().then((s) => appSettings.set(s)).catch((e) => (error = String(e)));
    currentRecording().then((id) => (recording = id !== null)).catch(() => {});
    const progress = onSessionProgress((p) => { recording = p.state === 'recording'; });
    lockedState()
      .then((reason) => {
        gate = reason ? { locked: reason } : 'open';
        if (!reason) takeEncryptionNotice()
            .then((n) => (notice = n))
            .catch((e) => (error = String(e)));
      })
      .catch((e) => (error = String(e)));
    appVersion()
      .then((v) => (version = v))
      .catch((e) => (error = String(e)));
    const unlisten = onHeartbeat((beat) => (beats = beat.seq));
    const stopLicense = followAccessState();
    return () => {
      window.removeEventListener('tray-start-failed', showRecord);
      unlisten.then((fn) => fn());
      progress.then((fn) => fn());
      modelEvents.then((fn) => fn());
      quitRequest.then((fn) => fn());
      trayStart.then((fn) => fn());
      stopLicense();
    };
  });
</script>

{#if typeof gate === 'object'}
  <LockedScreen reason={gate.locked} />
{:else if gate === 'open'}
<main>
  <BackgroundAnimation enabled={$appSettings.showBackgroundAnimation} {recording} />
  <header data-tauri-drag-region><span data-tauri-drag-region>Scribular</span></header>
  {#if confirmQuitOpen}<QuitConfirm close={() => (confirmQuitOpen = false)} />{/if}
  <UpdateBar />
  {#if notice}
    <p role="status">
      Your earlier test sessions were encrypted. Turn on FileVault to protect copies of the old file
      that macOS may still hold.
      <button onclick={() => (notice = false)}>Dismiss</button>
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
    <button onclick={(event) => navigate('settings', event.currentTarget)} aria-pressed={view === 'settings'} disabled={navigationDisabled(navigationLocked)} aria-disabled={navigationLocked}>Settings</button>
    <TrialBadge onclick={() => navigate('settings')} />
  </nav>
  <!-- Hidden, not unmounted, so a session in flight keeps its progress. -->
  <div hidden={view !== 'record'}><RecordView
      {speechReady}
      active={view === 'record'}
      onEnterLicense={() => navigate('settings')}
    />
  </div>
  {#if view === 'settings'}
    <Settings bind:this={settings} {navigationLocked} />
  {/if}
  <p>Heartbeat: {beats}</p>
</main>
{:else if error}
  <p role="alert">{error}</p>
{/if}
<style>
  header { height: 30px; line-height: 30px; padding-left: 70px; user-select: none; }
</style>
