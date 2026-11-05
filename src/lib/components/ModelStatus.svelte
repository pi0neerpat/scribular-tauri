<script lang="ts">
  import { onMount } from 'svelte';
  import { downloadModels, listSummaryModels, modelsStatus, onModelsProgress } from '../api';
  import { mergeSnapshot } from '../models';
  import type { ModelProgress } from '../types/ModelProgress';

  // Selection emits its current state; download ticks update the same snapshot.
  let summaryId = $state<string | null>(null);
  let models = $state<ModelProgress[]>([]);
  let llmIds = new Set<string>();
  let summary = $derived(models.find((m) => m.modelId === summaryId));

  onMount(() => {
    const selected = () =>
      listSummaryModels().then((list) => {
        llmIds = new Set(list.map((m) => m.spec.id));
        summaryId = list.find((m) => m.selected)?.spec.id ?? null;
      });
    const unlisten = onModelsProgress((p) => {
      models = [...models.filter((m) => m.modelId !== p.modelId), p];
      if (llmIds.has(p.modelId)) {
        if (p.state === 'failed') void selected();
        else summaryId = p.modelId;
      }
    });
    void selected();
    // Snapshot after the listener is live, so no progress event is missed;
    // entries already updated by an event are newer than the snapshot.
    unlisten
      .then(() => modelsStatus())
      .then((value) => (models = mergeSnapshot(value, models)));
    return () => {
      unlisten.then((fn) => fn());
    };
  });
</script>

<p role="status" aria-label="Summary model status">
  {#if summary?.state === 'ready'}
    Summary model ready
  {:else if summary?.state === 'failed'}
    Summary model download failed: {summary.error} <button onclick={downloadModels}>Retry download</button>
  {:else if !summary || summary.state === 'missing'}
    Summary model download has not started
  {:else}
    Summary model downloading{#if summary && summary.totalBytes > 0} ({Math.floor(summary.downloadedBytes / summary.totalBytes * 100)}%){/if}
  {/if}
</p>

