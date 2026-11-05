<script lang="ts">
  import { onMount } from 'svelte';
  import { appVersion, lockedState, onHeartbeat, takeEncryptionNotice } from './lib/api';
  import RecordView from './lib/components/RecordView.svelte';
  import Settings from './lib/components/Settings.svelte';
  import FirstRun from './lib/components/FirstRun.svelte';
  import ModelStatus from './lib/components/ModelStatus.svelte';
  import LockedScreen from './lib/components/LockedScreen.svelte';
  import TrialBadge from './lib/components/TrialBadge.svelte';
  import { followAccessState } from './lib/license';
  import type { AppVersion } from './lib/types/AppVersion';
  import type { LockReason } from './lib/types/LockReason';

  // Asked before anything that needs the store mounts.
  let gate = $state<'loading' | 'open' | { locked: LockReason }>('loading');
  let notice = $state(false);

  let version = $state<AppVersion | null>(null);
  let error = $state<string | null>(null);
  let beats = $state(0);
  let view = $state<'record' | 'settings'>('record');
  let speechReady = $state(false);
  let settings = $state<{ requestLeave: (action: () => void) => void } | undefined>();

  function navigate(next: typeof view) {
    const action = () => (view = next);
    if (view === 'settings' && next !== 'settings' && settings) settings.requestLeave(action);
    else action();
  }

  onMount(() => {
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
      unlisten.then((fn) => fn());
      stopLicense();
    };
  });
</script>

{#if typeof gate === 'object'}
  <LockedScreen reason={gate.locked} />
{:else if gate === 'open'}
<main>
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
    <button onclick={() => navigate('record')} aria-pressed={view === 'record'}>Record</button>
    <button onclick={() => navigate('settings')} aria-pressed={view === 'settings'}>Settings</button>
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
    <Settings bind:this={settings} />
  {/if}
  <p>Heartbeat: {beats}</p>
</main>
{:else if error}
  <p role="alert">{error}</p>
{/if}
