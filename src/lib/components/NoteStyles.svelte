<script lang="ts">
  // Note styles (parity spec 03 R12, R13): list, star, default, and an inline
  // editor that asks before dropping unsaved edits.
  import { onMount, tick } from 'svelte';
  import {
    createPrompt,
    deletePrompt,
    duplicatePrompt,
    getPromptList,
    setActivePrompt,
    setPromptStarred,
    updatePrompt,
  } from '../api';
  import type { Prompt } from '../types/Prompt';
  import type { PromptList } from '../types/PromptList';
  import NoteStyleEditor from './NoteStyleEditor.svelte';
  import StylePicker from './StylePicker.svelte';

  // `openId` of a new style not yet saved.
  const DRAFT = '';

  let list = $state<PromptList | null>(null);
  let openId = $state<string | null>(null);
  let name = $state('');
  let text = $state('');
  // The action waiting on the discard dialog.
  let pending = $state<(() => void) | null>(null);
  let deleting = $state<Prompt | null>(null);
  let error = $state<string | null>(null);
  let saving = $state(false);
  let dialogElement = $state<HTMLDivElement | undefined>();

  let open = $derived(list?.prompts.find((p) => p.id === openId) ?? null);
  let dirty = $derived(
    openId !== null && !open?.isDefault && (name !== (open?.name ?? '') || text !== (open?.text ?? '')),
  );
  let groups = $derived<[string, Prompt[]][]>([
    ['Custom', list?.prompts.filter((p) => !p.isDefault) ?? []],
    ['Built-in', list?.prompts.filter((p) => p.isDefault) ?? []],
  ]);

  onMount(() => { void call(getPromptList); });

  $effect(() => {
    if (pending || deleting) void tick().then(() => dialogElement?.querySelector('button')?.focus());
  });

  async function call(fn: () => Promise<PromptList>): Promise<boolean> {
    try {
      list = await fn();
      error = null;
      return true;
    } catch (e) {
      error = String(e);
      return false;
    }
  }

  function edit(id: string | null) {
    const p = list?.prompts.find((x) => x.id === id);
    openId = id;
    name = p?.name ?? '';
    text = p?.text ?? '';
  }

  function guard(action: () => void) {
    if (dirty) pending = action;
    else action();
  }

  async function save(): Promise<boolean> {
    if (saving) return false;
    saving = true;
    try {
      if (openId === DRAFT) {
        if (!(await call(() => createPrompt(name, text)))) return false;
        edit(list!.createdId);
        return true;
      }
      const id = openId!;
      if (!(await call(() => updatePrompt(id, name, text)))) return false;
      edit(id);
      return true;
    } finally {
      saving = false;
    }
  }

  async function duplicate(id: string) {
    if (await call(() => duplicatePrompt(id))) edit(list!.createdId);
  }

  async function confirmDelete(id: string) {
    deleting = null;
    if (await call(() => deletePrompt(id)) && openId === id) edit(null);
  }

  async function resolve(choice: 'save' | 'discard' | 'cancel') {
    const action = pending;
    pending = null;
    if (choice === 'cancel' || (choice === 'save' && !(await save()))) return;
    action?.();
  }
</script>

<section>
  <h3>Note styles</h3>
  {#if list}
    <StylePicker
      label="Default style"
      prompts={list.prompts}
      selectedId={list.activeId}
      onSelect={(id) => call(() => setActivePrompt(id))}
    />
    <button onclick={() => guard(() => edit(DRAFT))}>New style</button>
    {#if openId === DRAFT}
      <NoteStyleEditor bind:name bind:text readonly={false} {dirty} {saving} onSave={save} />
    {/if}
    {#each groups as [heading, prompts] (heading)}
      {#if prompts.length}
        <h4>{heading}</h4>
        <ul>
          {#each prompts as p (p.id)}
            <li>
              <button
                aria-label={`Toggle favourite for ${p.name}`}
                aria-pressed={p.starred}
                onclick={() => call(() => setPromptStarred(p.id, !p.starred))}>{p.starred ? '★' : '☆'}</button>
              <button aria-expanded={openId === p.id} onclick={() => guard(() => edit(openId === p.id ? null : p.id))}>
                {p.name}
              </button>
              {#if p.isDefault}<span>Built-in</span>{/if}
              {#if p.id === list.activeId}
                <span>Default</span>
              {:else}
                <button onclick={() => call(() => setActivePrompt(p.id))}>Make default</button>
              {/if}
              {#if p.description}<p>{p.description}</p>{/if}
              {#if openId === p.id}
                <NoteStyleEditor
                  bind:name
                  bind:text
                  readonly={p.isDefault}
                  {dirty}
                  {saving}
                  onSave={save}
                  onDuplicate={() => guard(() => duplicate(p.id))}
                  onDelete={() => guard(() => { edit(null); deleting = p; })}
                />
              {/if}
            </li>
          {/each}
        </ul>
      {/if}
    {/each}
  {/if}

  {#if pending}
    <div bind:this={dialogElement} role="dialog" aria-modal="true" aria-label="Discard changes?" tabindex="-1" onkeydown={(event) => event.key === 'Escape' && resolve('cancel')}>
      <p>Discard changes?</p>
      <button onclick={() => resolve('save')}>Save</button>
      <button onclick={() => resolve('discard')}>Discard</button>
      <button onclick={() => resolve('cancel')}>Cancel</button>
    </div>
  {/if}
  {#if deleting}
    {@const target = deleting}
    <div bind:this={dialogElement} role="dialog" aria-modal="true" aria-label="Delete style" tabindex="-1" onkeydown={(event) => event.key === 'Escape' && (deleting = null)}>
      <p>Delete “{target.name}”? This cannot be undone.</p>
      <button onclick={() => confirmDelete(target.id)}>Delete</button>
      <button onclick={() => (deleting = null)}>Cancel</button>
    </div>
  {/if}
  {#if error}<p role="alert">{error}</p>{/if}
</section>
