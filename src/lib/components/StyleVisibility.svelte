<script lang="ts">
  let { checked, disabled, label, onChange }:
    { checked: boolean; disabled: boolean; label: string; onChange: (checked: boolean) => void } = $props();
</script>

<button type="button" class="switch" role="switch" aria-label={label} aria-checked={checked} {disabled} onclick={() => onChange(!checked)}></button>

<style>
  .switch { position: relative; width: 2rem; height: 1.2rem; padding: 0; border-radius: 1rem; border: 1px solid #888; background: color-mix(in srgb, CanvasText 20%, Canvas); vertical-align: middle; }
  .switch::after { content: ''; position: absolute; width: .8rem; height: .8rem; top: .12rem; left: .14rem; border-radius: 50%; background: Canvas; }
  .switch[aria-checked='true'] { background: Highlight; }
  .switch[aria-checked='true']::after { left: 1rem; }
  .switch:disabled { opacity: 0.55; }
</style>
