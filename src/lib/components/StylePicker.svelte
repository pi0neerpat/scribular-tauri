<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { Prompt } from '../types/Prompt';

  let {
    prompts,
    selectedId,
    label = 'Note style',
    disabled = false,
    onSelect,
    coachEntry,
  }: {
    prompts: Prompt[];
    selectedId: string | null;
    label?: string;
    disabled?: boolean;
    onSelect: (id: string) => void;
    coachEntry?: Snippet;
  } = $props();

  let shown = $derived(prompts.filter((p) => p.showInStyleMenu));
  let selected = $derived(prompts.find((p) => p.id === selectedId));
  let menu = $state<HTMLDetailsElement>();
</script>

{#if coachEntry}
  <details bind:this={menu}>
    <summary>{label}{selected ? ` · ${selected.name}` : ''}</summary>
    <div class="choices">
      {#each shown as prompt (prompt.id)}
        <button disabled={disabled} aria-current={selectedId === prompt.id ? 'true' : undefined} onclick={() => { onSelect(prompt.id); if (menu) { menu.open = false; menu.querySelector('summary')?.focus(); } }}>{prompt.name}</button>
      {/each}
      {@render coachEntry()}
    </div>
  </details>
{:else}
  <label>
    {label}
    <select value={selectedId} {disabled} onchange={(e) => onSelect(e.currentTarget.value)}>
      {#if !shown.some((p) => p.id === selectedId)}
        <option value={selectedId ?? ''} disabled>{selected?.name ?? 'Choose a style'}</option>
      {/if}
      {#each shown as prompt (prompt.id)}
        <option value={prompt.id}>{prompt.name}</option>
      {/each}
    </select>
  </label>
{/if}

<style>
  details { display: inline-block; vertical-align: top; margin: 4px 0; }
  summary { cursor: pointer; }
  .choices { display: grid; gap: 5px; justify-items: start; border: 1px solid #8885; border-radius: 5px; padding: 6px; }
  [aria-current] { font-weight: bold; }
</style>
