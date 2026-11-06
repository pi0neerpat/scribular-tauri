<script lang="ts">
  import { onMount } from 'svelte';
  import { confirmQuit } from '../api';
  let { close }: { close: () => void } = $props();
  let dialog: HTMLDivElement;
  onMount(() => {
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    dialog.focus();
    return () => previous?.focus();
  });
  function keydown(event: KeyboardEvent) {
    if (event.key === 'Escape') { event.preventDefault(); close(); }
    if (event.key !== 'Tab') return;
    const buttons = Array.from(dialog.querySelectorAll<HTMLButtonElement>('button:not(:disabled)'));
    const first = buttons[0];
    const last = buttons.at(-1);
    if (event.shiftKey && (document.activeElement === first || document.activeElement === dialog)) { event.preventDefault(); last?.focus(); }
    else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first?.focus(); }
  }
</script>

<div role="presentation" class="veil">
  <div bind:this={dialog} role="alertdialog" aria-modal="true" aria-labelledby="quit-title" tabindex="-1" onkeydown={keydown}>
    <h2 id="quit-title">A recording is in progress</h2>
    <p>Quit and keep what's recorded so far?</p>
    <button onclick={close}>Cancel</button>
    <button onclick={() => confirmQuit()}>Quit</button>
  </div>
</div>

<style>
  .veil { position: fixed; inset: 0; display: grid; place-items: center; background: #0008; z-index: 10; }
  [role="alertdialog"] { background: Canvas; color: CanvasText; padding: 1.5rem; border-radius: .75rem; }
</style>
