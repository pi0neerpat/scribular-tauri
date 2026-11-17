<script lang="ts">
  // Starred styles, plus `includeId` (a session's own style) even if unstarred.
  import type { Prompt } from '../types/Prompt';

  let {
    prompts,
    selectedId,
    includeId = null,
    label = 'Note style',
    disabled = false,
    onSelect,
  }: {
    prompts: Prompt[];
    selectedId: string | null;
    includeId?: string | null;
    label?: string;
    disabled?: boolean;
    onSelect: (id: string) => void;
  } = $props();

  let shown = $derived(prompts.filter((p) => p.starred || p.id === includeId || p.id === selectedId));
</script>

<label>
  {label}
  <select value={selectedId} {disabled} onchange={(e) => onSelect(e.currentTarget.value)}>
    {#each shown as prompt (prompt.id)}
      <option value={prompt.id}>{prompt.name}</option>
    {/each}
  </select>
</label>
