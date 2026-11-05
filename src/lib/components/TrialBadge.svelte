<script lang="ts">
  import { accessState, badgeText } from '../license';

  let { onclick }: { onclick: () => void } = $props();
</script>

{#if $accessState}
  <button {onclick} data-testid="license-badge">{badgeText($accessState)}</button>
{/if}
