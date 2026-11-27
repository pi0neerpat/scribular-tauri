<script lang="ts">
  let { loadFailed = false, retrying = false, onRetry }: { loadFailed?: boolean; retrying?: boolean; onRetry?: () => void } = $props();
</script>

{#if loadFailed}
  <p>Summaries couldn't load. Recording and transcription still work.</p>
  {#if onRetry}<button disabled={retrying} onclick={onRetry}>Try Again</button>{/if}
{:else}
  <p>Summaries aren't available on this Mac. Recording and transcription still work.</p>
{/if}
