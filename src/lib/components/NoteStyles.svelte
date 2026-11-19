<script lang="ts">
  import { onMount, tick } from 'svelte';
  import {
    createPrompt,
    deletePrompt,
    duplicatePrompt,
    getPromptList,
    setPromptVisibility,
    openSettings,
    updatePrompt,
  } from '../api';
  import type { Prompt } from '../types/Prompt';
  import type { PromptList } from '../types/PromptList';
  import NoteStyleEditor from './NoteStyleEditor.svelte';
  import CoachEntry from './CoachEntry.svelte';
  import StyleVisibility from './StyleVisibility.svelte';
  import LicenseGate from './LicenseGate.svelte';
  import { accessState, allows } from '../license';
  import { getCoachContext } from '../coach-context';

  const coach = getCoachContext();
  const availability = $derived(coach?.availability() ?? null);

  // `openId` of a new style not yet saved.
  const DRAFT = '';

  let list = $state<PromptList | null>(null);
  let openId = $state<string | null>(null);
  let name = $state('');
  let text = $state('');
  let showInStyleMenu = $state(true);
  let detachDecision = $state<((allow: boolean) => void) | null>(null);
  // The action waiting on the discard dialog.
  let pending = $state<(() => void) | null>(null);
  let deleting = $state<Prompt | null>(null);
  let error = $state<string | null>(null);
  let saving = $state(false);
  let dialogElement = $state<HTMLDivElement | undefined>();
  let dialogOpener = $state<HTMLElement | null>(null);
  let dialogFallback = $state<HTMLElement | null>(null);

  let open = $derived(list?.prompts.find((p) => p.id === openId) ?? null);
  let dirty = $derived(
    openId !== null && !open?.isDefault && (name !== (open?.name ?? '') || text !== (open?.text ?? '') || showInStyleMenu !== (open?.showInStyleMenu ?? true)),
  );
  let groups = $derived<[string, Prompt[]][]>([
    ['Custom', list?.prompts.filter((p) => !p.isDefault) ?? []],
    ['Built-in', list?.prompts.filter((p) => p.isDefault) ?? []],
  ]);

  onMount(() => { void call(getPromptList, false); });

  $effect(() => {
    const updated = coach?.styles();
    if (updated) list = updated;
  });

  $effect(() => {
    if (pending || deleting || detachDecision) {
      const selector = pending && saving && !detachDecision ? '[data-dialog-save]' : '[data-dialog-initial-focus]';
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

  async function call(fn: () => Promise<PromptList>, broadcast = true): Promise<boolean> {
    try {
      list = await fn();
      if (broadcast) coach?.updateStyles(list);
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
    showInStyleMenu = p?.showInStyleMenu ?? true;
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
          if (!(await call(() => createPrompt(name, text, showInStyleMenu)))) return false;
          edit(list!.createdId);
          return true;
        }
        const id = openId!;
        const detaches = !!open?.styleSpec && text !== open.text;
        if (detaches && !(await new Promise<boolean>((resolve) => (detachDecision = resolve)))) return false;
        if (!(await call(() => updatePrompt(id, name, text, showInStyleMenu, detaches)))) return false;
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
    if (choice === 'discard') edit(open?.id ?? null);
    pending = null;
    const opener = dialogOpener;
    dialogOpener = null;
    if (opener?.isConnected) opener.focus();
    action?.();
  }

  function confirmDetach(allow: boolean) {
    const resolve = detachDecision;
    detachDecision = null;
    resolve?.(allow);
    if (!pending) void tick().then(() => document.querySelector<HTMLElement>('[data-save-style]')?.focus());
  }

  function toggleVisibility(p: Prompt, value: boolean) {
    if (p.id === openId && dirty) {
      showInStyleMenu = value;
      return;
    }
    void call(() => setPromptVisibility(p.id, value)).then((ok) => {
      if (ok && p.id === openId) showInStyleMenu = value;
    });
  }

  function dialogKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      if (detachDecision) confirmDetach(false);
      else if (pending) void resolve('cancel');
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
  <div inert={!!(pending || deleting || detachDecision)}>
  <h3>Note styles</h3>
  <LicenseGate access={$accessState} onEnter={() => { void openSettings('License').catch((e) => (error = String(e))); }} showWarning={false} />
  {#if list}
    {#if coach && groups[0][1].length}
      <CoachEntry {availability} onOpen={(opener) => coach.open({}, opener)} showRetry={availability?.retry === 'tryAgain'} />
    {/if}
    {#if coach && !groups[0][1].length}
      <aside class="invitation">
        <h4>Make notes that look like yours</h4>
        <p>Give the coach an example of the way you already write. Your example stays on this Mac.</p>
        <CoachEntry {availability} onOpen={(opener) => coach.open({}, opener)} showRetry={availability?.retry === 'tryAgain'} />
        <CoachEntry {availability} label="Use a Sample Note" onOpen={(opener) => coach.open({ samples: true }, opener)} />
      </aside>
    {/if}
    <button disabled={!allows($accessState)} onclick={(event) => guard(() => edit(DRAFT), event.currentTarget)}>New style</button>
    {#if openId === DRAFT}
      <NoteStyleEditor bind:name bind:text bind:showInStyleMenu readonly={!allows($accessState)} {dirty} {saving} onSave={save} />
    {/if}
    {#each groups as [heading, prompts] (heading)}
      {#if prompts.length}
        <h4>{heading}</h4>
        <div class="visibility-heading">Show in style menu</div>
        <ul>
          {#each prompts as p (p.id)}
            <li>
              <div class="style-row">
                <div>
                  <span>{p.name}</span>
                  {#if p.styleSpec}<small>Made with the coach</small>{/if}
                  <button aria-label={p.isDefault ? p.name : `Edit ${p.name}`} aria-expanded={openId === p.id} onclick={(event) => guard(() => edit(openId === p.id ? null : p.id), event.currentTarget)}>
                    {p.isDefault || !allows($accessState) ? 'View' : 'Edit'}
                  </button>
                  {#if coach && p.styleSpec && !p.isDefault}
                    <CoachEntry {availability} label="Adjust with the Coach…" onOpen={(opener) => coach.open({ styleId: p.id }, opener)} />
                  {/if}
                </div>
                <StyleVisibility checked={p.id === openId ? showInStyleMenu : p.showInStyleMenu} disabled={!allows($accessState) || saving} label={`Show ${p.name} in style menu`} onChange={(value) => toggleVisibility(p, value)} />
              </div>
              {#if p.description}<p>{p.description}</p>{/if}
              {#if openId === p.id}
                <NoteStyleEditor
                  bind:name
                  bind:text
                  bind:showInStyleMenu
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

  </div>
  {#if detachDecision}
    <div bind:this={dialogElement} role="dialog" aria-modal="true" aria-label="Save as a hand-edited style?" tabindex="-1" onkeydown={dialogKeydown}>
      <p>Save as a hand-edited style?</p>
      <p>Changing Note Style Text removes Adjust with the Coach from this style.</p>
      <button onclick={() => confirmDetach(true)}>Save text change</button>
      <button onclick={() => confirmDetach(false)} data-dialog-initial-focus>Cancel</button>
    </div>
  {:else if pending}
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


<style>
  .style-row { display: flex; justify-content: space-between; align-items: start; gap: 8px; }
  .style-row > div { display: flex; flex-wrap: wrap; align-items: center; gap: 5px; }
  .visibility-heading { text-align: right; font-size: .85rem; }
  .invitation { padding: 10px; border: 1px solid #8885; border-radius: 6px; margin-block: 8px; }
  .invitation h4 { margin-top: 0; }
  li { margin-block: 8px; }
</style>
