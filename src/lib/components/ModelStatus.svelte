<script lang="ts">
  import { onMount } from 'svelte';
  import { downloadModels, listSummaryModels, modelsStatus, onModelsProgress, onSummaryModelChanged } from '../api';
  import { mergeSnapshot } from '../models';
  import type { ModelProgress } from '../types/ModelProgress';

  // The selected summary model, refreshed only after a switch or rollback.
  let summaryId = $state<string | null>(null);
  let models = $state<ModelProgress[]>([]);
  let summary = $derived(models.find((m) => m.modelId === summaryId));
  let selectionVersion = 0;

  onMount(() => {
    const selected = () => {
      const version = selectionVersion;
      return listSummaryModels().then((list) => {
        if (version === selectionVersion) summaryId = list.find((m) => m.selected)?.spec.id ?? null;
      });
    };
    const unlisten = onModelsProgress((p) => {
      models = [...models.filter((m) => m.modelId !== p.modelId), p];
    });
    const changed = onSummaryModelChanged((id) => {
      selectionVersion++;
      summaryId = id;
    });
    void changed.then(selected, selected).catch(() => {});
    // Snapshot after the listener is live, so no progress event is missed;
    // entries already updated by an event are newer than the snapshot.
    void unlisten
      .then(() => modelsStatus(), () => modelsStatus())
      .then((value) => (models = mergeSnapshot(value, models)))
      .catch(() => {});
    return () => {
      unlisten.then((fn) => fn(), () => {});
      changed.then((fn) => fn(), () => {});
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

