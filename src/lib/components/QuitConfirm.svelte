<script lang="ts">
  import { onMount } from 'svelte';
  import { confirmQuit } from '../api';
  let { close, restart = false }: { close: () => void; restart?: boolean } = $props();
  let error = $state<string | null>(null);
  let confirming = $state(false);
  let dialog: HTMLDivElement;
  onMount(() => {
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    dialog.focus();
    return () => previous?.focus();
  });
  function keydown(event: KeyboardEvent) {
    if (event.key === 'Escape' && !confirming) { event.preventDefault(); close(); }
    if (event.key !== 'Tab') return;
    const buttons = Array.from(dialog.querySelectorAll<HTMLButtonElement>('button:not(:disabled)'));
    const first = buttons[0];
    const last = buttons.at(-1);
    if (event.shiftKey && (document.activeElement === first || document.activeElement === dialog)) { event.preventDefault(); last?.focus(); }
    else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first?.focus(); }
  }
  async function confirm() {
    confirming = true;
    error = null;
    try { await confirmQuit(restart); }
    catch (e) { error = String(e); }
    finally { confirming = false; }
  }
</script>

<div role="presentation" class="veil">
  <div bind:this={dialog} role="alertdialog" aria-modal="true" aria-labelledby="quit-title" tabindex="-1" onkeydown={keydown}>
    <h2 id="quit-title">{restart ? 'Restart to update?' : 'A recording is in progress'}</h2>
    <p>{restart ? 'Restarting will interrupt the work in progress. Scribular will keep what it has recorded so far.' : "Quit and keep what's recorded so far?"}</p>
    <button onclick={close} disabled={confirming}>Cancel</button>
    <button onclick={confirm} disabled={confirming}>{restart ? 'Restart Now' : 'Quit'}</button>
    {#if error}<p role="alert">{error}</p>{/if}
  </div>
</div>

<style>
  .veil { position: fixed; inset: 0; display: grid; place-items: center; background: #0008; z-index: 10; }
  [role="alertdialog"] { background: Canvas; color: CanvasText; padding: 1.5rem; border-radius: .75rem; }
</style>
