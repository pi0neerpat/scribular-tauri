<script lang="ts">
  import { onMount } from 'svelte';
  import { appVersion, onHeartbeat } from './lib/api';
  import RecordView from './lib/components/RecordView.svelte';
  import Settings from './lib/components/Settings.svelte';
  import type { AppVersion } from './lib/types/AppVersion';

  let version = $state<AppVersion | null>(null);
  let error = $state<string | null>(null);
  let beats = $state(0);
  let view = $state<'record' | 'settings'>('record');

  onMount(() => {
    appVersion()
      .then((v) => (version = v))
      .catch((e) => (error = String(e)));
    const unlisten = onHeartbeat((beat) => (beats = beat.seq));
    return () => {
      unlisten.then((fn) => fn());
    };
  });
</script>

<main>
  {#if version}
    <h1>{version.name} <span data-testid="version">{version.version}</span></h1>
  {:else if error}
    <p role="alert">{error}</p>
  {:else}
    <p>Loading…</p>
  {/if}
  <nav>
    <button onclick={() => (view = 'record')} aria-pressed={view === 'record'}>Record</button>
    <button onclick={() => (view = 'settings')} aria-pressed={view === 'settings'}>Settings</button>
  </nav>
  <!-- Hidden, not unmounted, so a session in flight keeps its progress. -->
  <div hidden={view !== 'record'}><RecordView /></div>
  {#if view === 'settings'}
    <Settings />
  {/if}
  <p>Heartbeat: {beats}</p>
</main>
