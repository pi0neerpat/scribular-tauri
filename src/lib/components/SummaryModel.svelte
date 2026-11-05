<script lang="ts">
  // Summary model picker (parity spec 03 R19, R23).
  import { onMount } from 'svelte';
  import { listSummaryModels, onModelsProgress, openExternalLink, setSummaryModel } from '../api';
  import type { SummaryModel } from '../types/SummaryModel';

  let models = $state<SummaryModel[]>([]);
  let picked = $state<string | null>(null);
  let confirming = $state(false);
  let error = $state<string | null>(null);

  let selected = $derived(models.find((m) => m.selected) ?? null);
  let choice = $derived(models.find((m) => m.spec.id === picked) ?? null);

  const gb = (bytes: number) => `${(bytes / 1e9).toFixed(1)} GB`;

  async function refresh() {
    try {
      models = await listSummaryModels();
      picked ??= models.find((m) => m.selected)?.spec.id ?? null;
    } catch (e) {
      error = String(e);
    }
  }

  onMount(() => {
    // A failed download reverts the selection, so re-read it on each event.
    const unlisten = onModelsProgress((p) => {
      if (models.some((m) => m.spec.id === p.modelId)) void refresh();
    });
    unlisten.then(refresh, refresh);
    return () => { unlisten.then((fn) => fn(), () => {}); };
  });

  async function use() {
    if (!choice) return;
    if (choice.state !== 'ready' && !confirming) {
      confirming = true;
      return;
    }
    confirming = false;
    try {
      await setSummaryModel(choice.spec.id);
      error = null;
    } catch (e) {
      error = String(e);
    }
    await refresh();
  }
</script>

<fieldset>
  <legend>Summary model</legend>
  {#each models as m (m.spec.id)}
    <label>
      <input type="radio" name="summary-model" checked={picked === m.spec.id} onchange={() => { picked = m.spec.id; confirming = false; }} />
      {m.spec.displayName} · {gb(m.spec.size)}
      {#if m.state === 'ready'}<span>Installed</span>
      {:else if m.state === 'downloading' || m.state === 'verifying'}<span>Downloading</span>
      {:else if m.state === 'failed'}<span>Download failed</span>{/if}
    </label>
    <p>{m.spec.description}</p>
    {#if m.spec.caution}<p>{m.spec.caution}</p>{/if}
    <button class="link" onclick={() => openExternalLink(m.spec.licenseUrl)}>{m.spec.license} licence</button>
  {/each}
  {#if confirming && choice}
    <p role="status">Downloads {gb(choice.spec.size)} once. Licence: {choice.spec.license}.</p>
  {/if}
  <button onclick={use} disabled={!choice || choice.spec.id === selected?.spec.id}>
    {confirming ? 'Download and use' : 'Use this model'}
  </button>
  {#if error}<p role="alert">{error}</p>{/if}
</fieldset>
