<script lang="ts">
  // Inline editor for one style. Built-ins are read-only.
  let {
    name = $bindable(),
    text = $bindable(),
    readonly,
    dirty,
    saving = false,
    onSave,
    onDuplicate,
    onDelete,
  }: {
    name: string;
    text: string;
    readonly: boolean;
    dirty: boolean;
    saving?: boolean;
    onSave: () => void;
    onDuplicate?: () => void;
    onDelete?: () => void;
  } = $props();
</script>

<div>
  <label>Name <input bind:value={name} {readonly} /></label>
  <label>Prompt <textarea bind:value={text} {readonly} rows="10"></textarea></label>
  {#if readonly}
    {#if onDuplicate}<button onclick={onDuplicate}>Duplicate to edit</button>{/if}
  {:else}
    <button onclick={onSave} disabled={!dirty || !text.trim() || saving}>Save</button>
    {#if dirty}<span>Unsaved changes</span>{/if}
    {#if onDelete}<button onclick={onDelete}>Delete</button>{/if}
  {/if}
</div>
