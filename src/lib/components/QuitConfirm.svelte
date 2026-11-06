<script lang="ts">
  import { confirmQuit } from '../api';
  let { close }: { close: () => void } = $props();
</script>

<div role="presentation" class="veil">
  <div role="alertdialog" aria-modal="true" aria-labelledby="quit-title" tabindex="-1">
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
