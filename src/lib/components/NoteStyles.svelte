<script lang="ts">
  // Note styles (parity spec 03 R12, R13): list, star, default, and an inline
  // editor that asks before dropping unsaved edits.
  import { onMount, tick } from 'svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import {
    createPrompt,
    deletePrompt,
    duplicatePrompt,
    getPromptList,
    setActivePrompt,
    setPromptStarred,
    openSettings,
    retrySummaryModel,
    subscribeSummaryAvailability,
    updatePrompt,
  } from '../api';
  import type { Prompt } from '../types/Prompt';
  import type { PromptList } from '../types/PromptList';
  import NoteStyleEditor from './NoteStyleEditor.svelte';
  import StylePicker from './StylePicker.svelte';
  import LicenseGate from './LicenseGate.svelte';
  import SummaryUnavailable from './SummaryUnavailable.svelte';
  import { accessState, allows } from '../license';
  import type { SummaryAvailability } from '../types/SummaryAvailability';

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
  let summaryAvailability = $state<SummaryAvailability>('downloading');
  let summaryError = $state(false);
  let dialogElement = $state<HTMLDivElement | undefined>();
  let dialogOpener = $state<HTMLElement | null>(null);
  let dialogFallback = $state<HTMLElement | null>(null);

  let open = $derived(list?.prompts.find((p) => p.id === openId) ?? null);
  let dirty = $derived(
    openId !== null && !open?.isDefault && (name !== (open?.name ?? '') || text !== (open?.text ?? '')),
  );
  let groups = $derived<[string, Prompt[]][]>([
    ['Custom', list?.prompts.filter((p) => !p.isDefault) ?? []],
    ['Built-in', list?.prompts.filter((p) => p.isDefault) ?? []],
  ]);

  let disposed = false;
  let unlistenSummary: (() => void) | undefined;
  function watchSummary() {
    unlistenSummary?.();
    unlistenSummary = undefined;
    void subscribeSummaryAvailability((state) => { if (!disposed) { summaryAvailability = state; summaryError = false; } }, () => { if (!disposed) summaryError = true; })
      .then((remove) => { if (disposed) remove(); else unlistenSummary = remove; })
      .catch(() => { if (!disposed) summaryError = true; });
  }

  onMount(() => {
    void call(getPromptList);
    let unlisten: (() => void) | undefined;
    watchSummary();
    void getCurrentWindow().onCloseRequested((event) => {
      if (!dirty) return;
      event.preventDefault();
      dialogOpener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
      pending = () => { void getCurrentWindow().destroy(); };
    }).then((removeListener) => {
      if (disposed) removeListener();
      else unlisten = removeListener;
    });
    return () => { disposed = true; unlisten?.(); unlistenSummary?.(); };
  });

  $effect(() => {
    if (pending || deleting) {
      const selector = pending && saving ? '[data-dialog-save]' : '[data-dialog-initial-focus]';
      void tick().then(() => dialogElement?.querySelector<HTMLElement>(selector)?.focus());
    } else if (dialogOpener) {
      const opener = dialogOpener;
      const fallback = dialogFallback;
      dialogOpener = null;
      dialogFallback = null;
      void tick().then(() => {
        if (opener.isConnected) opener.focus();
        else fallback?.focus();
      });
    }
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

  function guard(action: () => void, opener = document.activeElement) {
    if (dirty) {
      dialogOpener = opener instanceof HTMLElement ? opener : null;
      pending = action;
    }
    else action();
  }

  export function requestLeave(action: () => void, opener?: HTMLElement) {
    guard(action, opener);
  }

  export function newBlank() { if (allows($accessState)) guard(() => { edit(DRAFT); void tick().then(() => document.querySelector<HTMLElement>('[data-style-name]')?.focus()); }); }

  let savePromise: Promise<boolean> | null = null;

  function save(): Promise<boolean> {
    if (savePromise) return savePromise;
    if (!allows($accessState)) return Promise.resolve(false);
    saving = true;
    savePromise = (async () => {
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
        savePromise = null;
      }
    })();
    return savePromise;
  }

  async function duplicate(id: string) {
    if (await call(() => duplicatePrompt(id))) edit(list!.createdId);
  }

  async function confirmDelete(id: string) {
    const fallback = dialogFallback;
    deleting = null;
    const deleted = await call(() => deletePrompt(id));
    if (deleted && openId === id) edit(null);
    if (deleted && fallback?.isConnected) {
      await tick();
      fallback.focus();
    }
  }

  async function resolve(choice: 'save' | 'discard' | 'cancel') {
    const action = pending;
    if (choice === 'cancel') { pending = null; return; }
    if (choice === 'save' && !(await save())) return;
    pending = null;
    const opener = dialogOpener;
    dialogOpener = null;
    if (opener?.isConnected) opener.focus();
    action?.();
  }

  function dialogKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      if (pending) void resolve('cancel');
      else deleting = null;
    } else if (event.key === 'Tab') {
      const buttons = [...(dialogElement?.querySelectorAll<HTMLButtonElement>('button:not(:disabled)') ?? [])];
      const first = buttons[0];
      const last = buttons.at(-1);
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last?.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first?.focus();
      }
    }
  }
</script>

<section>
  <h3>Note styles</h3>
  <LicenseGate access={$accessState} onEnter={() => { void openSettings('License').catch((e) => (error = String(e))); }} showWarning={false} />
  {#if summaryAvailability === 'unsupported'}
    <SummaryUnavailable />
  {:else if summaryAvailability === 'loadFailed'}
    <p>The summary model couldn't load.</p>
    <button onclick={() => { void retrySummaryModel().catch(() => (summaryError = true)); }}>Try again</button>
  {:else if summaryAvailability === 'downloading' && !summaryError}
    <p>Waiting for the summary model download…</p>
  {/if}
  {#if summaryError}<p role="alert">Couldn't check summary availability. <button onclick={() => { summaryError = false; watchSummary(); }}>Try again</button></p>{/if}
  {#if list}
    <StylePicker
      label="Default style"
      prompts={list.prompts}
      selectedId={list.activeId}
      disabled={!allows($accessState)}
      onSelect={(id) => { if (allows($accessState)) void call(() => setActivePrompt(id)); }}
    />
    <button disabled={!allows($accessState)} onclick={(event) => guard(() => edit(DRAFT), event.currentTarget)}>New style</button>
    {#if openId === DRAFT}
      <NoteStyleEditor bind:name bind:text readonly={!allows($accessState)} {dirty} {saving} onSave={save} />
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
                disabled={!allows($accessState)}
                onclick={() => call(() => setPromptStarred(p.id, !p.starred))}>{p.starred ? '★' : '☆'}</button>
              <button aria-expanded={openId === p.id} onclick={(event) => guard(() => edit(openId === p.id ? null : p.id), event.currentTarget)}>
                {p.name}
              </button>
              {#if p.isDefault}<span>Built-in</span>{/if}
              {#if p.id === list.activeId}
                <span>Default</span>
              {:else}
                <button disabled={!allows($accessState)} onclick={() => call(() => setActivePrompt(p.id))}>Make default</button>
              {/if}
              {#if p.description}<p>{p.description}</p>{/if}
              {#if openId === p.id}
                <NoteStyleEditor
                  bind:name
                  bind:text
                  readonly={p.isDefault || !allows($accessState)}
                  {dirty}
                  {saving}
                  onSave={save}
                  onDuplicate={allows($accessState) ? (opener) => guard(() => duplicate(p.id), opener) : undefined}
                  onDelete={p.isDefault ? undefined : (opener) => guard(() => {
                    dialogOpener = opener.closest('li')?.querySelector<HTMLElement>('button[aria-expanded]') ?? opener;
                    dialogFallback = opener.closest('section')?.querySelector<HTMLElement>('button') ?? null;
                    edit(null);
                    deleting = p;
                  }, opener)}
                />
              {/if}
            </li>
          {/each}
        </ul>
      {/if}
    {/each}
  {/if}

  {#if pending}
    <div bind:this={dialogElement} role="dialog" aria-modal="true" aria-label="Discard changes?" tabindex="-1" onkeydown={dialogKeydown}>
      <p>Discard changes?</p>
      <button onclick={() => resolve('save')} disabled={!allows($accessState)} data-dialog-save>Save</button>
      <button onclick={() => resolve('discard')} disabled={saving}>Discard</button>
      <button onclick={() => resolve('cancel')} disabled={saving} data-dialog-initial-focus>Cancel</button>
    </div>
  {/if}
  {#if deleting}
    {@const target = deleting}
    <div bind:this={dialogElement} role="dialog" aria-modal="true" aria-label="Delete style" tabindex="-1" onkeydown={dialogKeydown}>
      <p>Delete “{target.name}”? This cannot be undone.</p>
      <button onclick={() => confirmDelete(target.id)}>Delete</button>
      <button onclick={() => (deleting = null)} data-dialog-initial-focus>Cancel</button>
    </div>
  {/if}
  {#if error}<p role="alert">{error}</p>{/if}
</section>
