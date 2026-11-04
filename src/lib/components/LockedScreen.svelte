<script lang="ts">
  // Shown instead of the app when the encrypted store can't be opened (spec 01 R7).
  // Nothing else mounts, so no command that needs the store is ever called.
  import { quitApp, startFresh } from '../api';
  import type { LockReason } from '../types/LockReason';
  import { confirmsStartFresh, lockedCopy, START_FRESH_PHRASE } from './locked';

  let { reason }: { reason: LockReason } = $props();
  let copy = $derived(lockedCopy(reason));
  let confirming = $state(false);
  let typed = $state('');
  let error = $state<string | null>(null);

  async function confirm() {
    error = null;
    try {
      await startFresh(typed);
    } catch (e) {
      error = String(e);
    }
  }
</script>

<main>
  <h1>{copy.title}</h1>
  <p>{copy.body}</p>
  {#if confirming}
    <div role="dialog" aria-labelledby="fresh-title">
      <h2 id="fresh-title">Start fresh?</h2>
      <p>
        Your earlier sessions can't be recovered. The unreadable file is set aside, not shared, and
        Scribular starts with an empty history.
      </p>
      <label>
        Type “{START_FRESH_PHRASE}” to confirm
        <input bind:value={typed} autocomplete="off" />
      </label>
      <button onclick={confirm} disabled={!confirmsStartFresh(typed)}>Start fresh</button>
      <button onclick={() => ((confirming = false), (typed = ''))}>Cancel</button>
    </div>
  {:else}
    <button onclick={() => quitApp()}>Quit</button>
    {#if copy.canStartFresh}
      <button onclick={() => (confirming = true)}>Start fresh…</button>
    {/if}
  {/if}
  {#if error}<p role="alert">{error}</p>{/if}
</main>
