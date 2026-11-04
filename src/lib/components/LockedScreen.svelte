<script lang="ts">
  // Shown instead of the app when the encrypted store can't be opened (spec 01 R7).
  // Nothing else mounts, so no command that needs the store is ever called.
  import { tick } from 'svelte';
  import { quitApp, startFresh } from '../api';
  import type { LockReason } from '../types/LockReason';
  import { confirmsStartFresh, lockedCopy, START_FRESH_PHRASE } from './locked';

  let { reason }: { reason: LockReason } = $props();
  let copy = $derived(lockedCopy(reason));
  let confirming = $state(false);
  let typed = $state('');
  let error = $state<string | null>(null);

  let opener = $state<HTMLButtonElement | null>(null);

  // The opener only renders again once the dialog is gone, so focus it after the update.
  async function cancel() {
    confirming = false;
    typed = '';
    await tick();
    opener?.focus();
  }

  // Focus the phrase field when the dialog opens (the button that opened it is gone).
  function focusOnMount(el: HTMLInputElement) {
    el.focus();
  }

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
    <div
      role="dialog"
      aria-labelledby="fresh-title"
      tabindex="-1"
      onkeydown={(e) => e.key === 'Escape' && cancel()}
    >
      <h2 id="fresh-title">Start fresh?</h2>
      <p>
        Your earlier sessions can't be recovered. The unreadable file is kept on this Mac, not
        deleted, and Scribular starts with an empty history.
      </p>
      <label>
        Type “{START_FRESH_PHRASE}” to confirm
        <input bind:value={typed} autocomplete="off" use:focusOnMount />
      </label>
      <button onclick={confirm} disabled={!confirmsStartFresh(typed)}>Start fresh</button>
      <button onclick={cancel}>Cancel</button>
    </div>
  {:else}
    <button onclick={() => quitApp()}>Quit</button>
    {#if copy.canStartFresh}
      <button bind:this={opener} onclick={() => (confirming = true)}>Start fresh…</button>
    {/if}
  {/if}
  {#if error}<p role="alert">{error}</p>{/if}
</main>
