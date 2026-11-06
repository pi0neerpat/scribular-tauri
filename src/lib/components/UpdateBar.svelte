<script lang="ts">
  import { onMount } from 'svelte';
  import { getUpdateState, installUpdate, onUpdateState, openExternalLink } from '../api';
  import type { UpdateState } from '../types/UpdateState';

  let updateState = $state<UpdateState>({ state: 'idle', installable: false });
  let dismissedVersion = $state<string | null>(null);
  let error = $state<string | null>(null);

  // A new state or version retires the old error; a same-version error stays for retry.
  function apply(next: UpdateState) {
    if (next.state !== 'ready' || updateState.state !== 'ready' || next.version !== updateState.version) error = null;
    updateState = next;
  }

  function later() {
    if (updateState.state === 'ready') dismissedVersion = updateState.version;
  }

  onMount(() => {
    let remove: (() => void) | undefined;
    let disposed = false;
    let sawEvent = false;
    void (async () => {
      try {
        const unlisten = await onUpdateState((next) => { sawEvent = true; apply(next); });
        if (disposed) unlisten(); else remove = unlisten;
      } catch { /* The state snapshot still lets the bar render. */ }
      try {
        const snapshot = await getUpdateState();
        // An event that arrived meanwhile is newer than the snapshot.
        if (!sawEvent) apply(snapshot);
      } catch { /* updater unavailable */ }
    })();
    return () => { disposed = true; remove?.(); };
  });
</script>

{#if updateState.state === 'ready' && updateState.installable && dismissedVersion !== updateState.version}
  <aside role="status" aria-label="Update ready">
    <span>Scribular {updateState.version} is ready</span>
    <button onclick={() => installUpdate().catch((e) => (error = String(e)))}>Restart to update</button>
    <button onclick={later}>Later</button>
    <button onclick={() => openExternalLink('https://www.scribular.com/releases').catch((e) => (error = String(e)))}>What's new</button>
    {#if error}<p role="alert">{error}</p>{/if}
  </aside>
{/if}
