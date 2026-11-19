<script lang="ts">
  import StyleVisibility from './StyleVisibility.svelte';

  // Inline editor for one style. Built-ins are read-only.
  let {
    name = $bindable(),
    text = $bindable(),
    showInStyleMenu = $bindable(),
    showVisibility = false,
    readonly,
    dirty,
    saving = false,
    onSave,
    onDuplicate,
    onDelete,
  }: {
    name: string;
    text: string;
    showInStyleMenu: boolean;
    showVisibility?: boolean;
    readonly: boolean;
    dirty: boolean;
    saving?: boolean;
    onSave: () => void;
    onDuplicate?: (opener: HTMLElement) => void;
    onDelete?: (opener: HTMLElement) => void;
  } = $props();
</script>

<div>
  <label>Name <input data-style-name bind:value={name} {readonly} /></label>
  <label>Note Style Text <textarea bind:value={text} {readonly} rows="10"></textarea></label>
  {#if showVisibility}<div><StyleVisibility checked={showInStyleMenu} disabled={readonly || saving} label="Show in style menu" onChange={(value) => (showInStyleMenu = value)} /> Show in style menu</div>{/if}
  {#if readonly}
    {#if onDuplicate}<button onclick={(event) => onDuplicate(event.currentTarget)}>Duplicate to edit</button>{/if}
  {:else}
    <button data-save-style onclick={onSave} disabled={!dirty || !text.trim() || saving}>Save</button>
    {#if dirty}<span>Unsaved changes</span>{/if}
  {/if}
  {#if onDelete}<button onclick={(event) => onDelete(event.currentTarget)}>Delete</button>{/if}
</div>
