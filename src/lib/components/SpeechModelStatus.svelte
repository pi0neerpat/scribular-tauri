<script lang="ts">
  import { downloadModels } from '../api';
  import type { ModelProgress } from '../types/ModelProgress';

  let { speech }: { speech: ModelProgress | undefined } = $props();
</script>

<li>
  {#if speech?.state === 'failed'}
    Speech model download failed: {speech.error ?? 'Unknown download error.'}
    <button onclick={downloadModels}>Retry speech model download</button>
  {:else}
    Downloading the speech model{#if speech && speech.totalBytes > 0} ({Math.floor(speech.downloadedBytes / speech.totalBytes * 100)}%){/if}.
  {/if}
</li>
