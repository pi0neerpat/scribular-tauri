<script lang="ts">
  import { onMount } from 'svelte';
  import { getUpdateState, installUpdate, onUpdateState, showUpdateReadyAlert } from '../api';
  import type { UpdateState } from '../types/UpdateState';

  let { recording = false }: { recording?: boolean } = $props();
  let updateState = $state<UpdateState>({ state: 'idle', installable: false });
  let shownVersion = $state<string | null>(null);
  let error = $state<string | null>(null);
  let pending = $state(false);
  let attempted: UpdateState | null = null;

  $effect(() => {
    if (recording) {
      attempted = null;
      return;
    }
    if (pending || updateState.state !== 'ready' || attempted === updateState || shownVersion === updateState.version) return;
    const version = updateState.version;
    attempted = updateState;
    pending = true;
    error = null;
    void showUpdateReadyAlert(version).then((restart) => {
      if (restart !== null) shownVersion = version;
      if (restart) return installUpdate();
    }).catch((e) => {
      shownVersion = version;
      error = `Could not show or install the update: ${String(e)}. Open Settings → General to check for updates.`;
    }).finally(() => { pending = false; });
  });

  onMount(() => {
    let remove: (() => void) | undefined;
    let disposed = false;
    let revision = 0;
    void (async () => {
      try {
        const unlisten = await onUpdateState((next) => {
          revision++;
          // Keep the error only while the same ready version stays current, so it stays recoverable.
          if (!(next.state === 'ready' && updateState.state === 'ready' && next.version === updateState.version)) error = null;
          updateState = next;
        });
        if (disposed) unlisten(); else remove = unlisten;
      } catch (e) { if (!disposed) error = `Live update events are unavailable: ${String(e)}`; }
      const snapshotRevision = revision;
      try {
        const snapshot = await getUpdateState();
        if (revision === snapshotRevision && !disposed) updateState = snapshot;
      } catch (e) { if (!revision && !disposed) error = `Could not check update status: ${String(e)}`; }
    })();
    return () => { disposed = true; remove?.(); };
  });
</script>

{#if error}<p role="alert">{error} <button onclick={() => (error = null)}>Dismiss</button></p>{/if}
