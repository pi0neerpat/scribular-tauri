<script lang="ts">
  import { onMount } from 'svelte';
  import { getUpdateState, installUpdate, onUpdateState, openExternalLink } from '../api';
  import type { UpdateState } from '../types/UpdateState';

  let updateState = $state<UpdateState>({ state: 'idle', installable: false });
  let dismissedVersion = $state<string | null>(null);

  function later() {
    if (updateState.state === 'ready') dismissedVersion = updateState.version;
  }

  onMount(() => {
    let remove: (() => void) | undefined;
    let disposed = false;
    void (async () => {
      try {
        const unlisten = await onUpdateState((next) => (updateState = next));
        if (disposed) unlisten(); else remove = unlisten;
      } catch { /* The state snapshot still lets the bar render. */ }
      try { updateState = await getUpdateState(); } catch { /* updater unavailable */ }
    })();
    return () => { disposed = true; remove?.(); };
  });
</script>

{#if updateState.state === 'ready' && updateState.installable && dismissedVersion !== updateState.version}
  <aside role="status" aria-label="Update ready">
    <span>Scribular {updateState.version} is ready</span>
    <button onclick={() => installUpdate()}>Restart to update</button>
    <button onclick={later}>Later</button>
    <button onclick={() => openExternalLink('https://www.scribular.com/releases')}>What's new</button>
  </aside>
{/if}
