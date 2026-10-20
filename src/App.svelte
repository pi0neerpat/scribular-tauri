<script lang="ts">
  import { onMount } from 'svelte';
  import { appVersion, onHeartbeat } from './lib/api';
  import type { AppVersion } from './lib/types/AppVersion';

  let version = $state<AppVersion | null>(null);
  let error = $state<string | null>(null);
  let beats = $state(0);

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
  <p>Heartbeat: {beats}</p>
</main>
