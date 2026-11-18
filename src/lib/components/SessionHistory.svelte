<script lang="ts">
  import { onMount, type Snippet } from 'svelte';
  import { cancelJob, clearAllSessions, deleteSession, getPromptList, getSession, listSessions, listSummaryModels, onSessionProgress, openSettings, recoverSession, searchSessions, setShellMenuState } from '../api';
  import type { AppView } from '../navigation';
  import type { Session } from '../types/Session';
  import type { SessionListItem } from '../types/SessionListItem';
  import type { SessionProgress } from '../types/SessionProgress';
  import SummaryPanel from './SummaryPanel.svelte';
  import TrialBadge from './TrialBadge.svelte';
  import { accessState, allows } from '../license';

  let { view = 'history', navigationLocked = false, recording = false, recoverable = false, recordHasNote = false, onNavigate = () => {}, onLicense = () => {}, onTour = () => {}, onSessionsChanged, children }: {
    view?: AppView; navigationLocked?: boolean; recording?: boolean; recoverable?: boolean; recordHasNote?: boolean; onNavigate?: (view: AppView, opener?: HTMLElement, action?: () => void) => void;
    onLicense?: () => void; onTour?: () => void; onSessionsChanged?: () => void; children?: Snippet;
  } = $props();
  let sessions = $state<SessionListItem[]>([]);
  let selected = $state<Session | null>(null);
  let progress = $state<Record<string, SessionProgress>>({});
  let error = $state<string | null>(null);
  let clearResult = $state('');
  let clearing = $state(false);
  let query = $state('');
  let searchField: HTMLInputElement;
  let request = 0;
  let copiedId = $state<string | null>(null);
  let copiedTimer: ReturnType<typeof setTimeout> | undefined;
  let deleteConfirmId = $state<string | null>(null);
  let summaryPanel = $state<{ copy: () => Promise<void>; requestDelete: () => void } | undefined>();
  let deleteTimer: ReturnType<typeof setTimeout> | undefined;
  let prompts = $state<Awaited<ReturnType<typeof getPromptList>> | null>(null);
  let models = $state<Awaited<ReturnType<typeof listSummaryModels>>>([]);
  let interrupted = $derived(sessions.filter((s) => s.interrupted && s.hasAudio && s.status === 'failed').length);
  $effect(() => {
    // Refresh native availability after a list change, even when search hides every row.
    sessions; $accessState;
    void setShellMenuState({ selected: view === 'history' && !!selected && !['recording', 'finalizing'].includes(selected.status), hasNote: view === 'history' ? !!selected?.summary : view === 'record' && recordHasNote, recording, recoverable, locked: navigationLocked }).catch(() => {});
  });
  let groups = $derived.by(() => {
    const today = new Date(); today.setHours(0, 0, 0, 0);
    const yesterday = new Date(today); yesterday.setDate(today.getDate() - 1);
    return [
      { label: 'Today', rows: sessions.filter((s) => s.createdAt >= today.getTime()) },
      { label: 'Yesterday', rows: sessions.filter((s) => s.createdAt >= yesterday.getTime() && s.createdAt < today.getTime()) },
      { label: 'Earlier', rows: sessions.filter((s) => s.createdAt < yesterday.getTime()) },
    ];
  });

  async function refresh() {
    const current = ++request;
    try {
      const result = query ? await searchSessions(query) : await listSessions();
      if (current !== request) return;
      sessions = result;
      error = null;
      if (!query && selected && !result.some((s) => s.id === selected?.id)) selected = null;
    } catch (e) { if (current === request) error = String(e); }
  }

  function open(item: SessionListItem, opener: HTMLElement) {
    onNavigate('history', opener, () => {
      void getSession(item.id).then((session) => { selected = session; }).catch((e) => (error = String(e)));
    });
  }

  export function focusSearch() {
    searchField?.focus();
    searchField?.select();
  }
  export function confirmDeleteSelected() { if (view === 'history' && selected && !['recording', 'finalizing'].includes(selected.status)) summaryPanel?.requestDelete(); }
  export async function copySelectedNote() { if (view === 'history' && selected?.summary) await summaryPanel?.copy(); }
  export async function recoverFirst() {
    try {
      const first = (await listSessions()).find((s) => s.interrupted && s.hasAudio && s.status === 'failed');
      if (first) await recover(first.id);
    } catch (e) { error = String(e); }
  }
  export function refreshSessions() { void refresh(); }

  function markCopied(id: string) {
    copiedId = id;
    clearTimeout(copiedTimer);
    copiedTimer = setTimeout(() => (copiedId = null), 2000);
  }

  async function applyProgress(p: SessionProgress) {
    progress[p.sessionId] = p;
    const index = sessions.findIndex((s) => s.id === p.sessionId);
    if (index < 0 || ['done', 'failed', 'cancelled'].includes(p.state)) {
      await refresh();
    } else {
      sessions = sessions.map((s) => s.id === p.sessionId
        ? { ...s, status: p.state, snippet: p.state === 'summarizing' ? 'Summarizing…' : s.snippet }
        : s);
    }
    if (selected?.id === p.sessionId) selected = await getSession(p.sessionId);
  }

  onMount(() => {
    void refresh();
    getPromptList().then((p) => (prompts = p)).catch(() => {});
    listSummaryModels().then((m) => (models = m)).catch(() => {});
    const unlisten = onSessionProgress((p) => { void applyProgress(p).catch((e) => (error = String(e))); });
    return () => { unlisten.then((fn) => fn()); clearTimeout(deleteTimer); clearTimeout(copiedTimer); };
  });

  async function recover(id: string) {
    error = null;
    try { await recoverSession(id); await refresh(); }
    catch (e) { error = String(e); }
  }

  async function cancel(id: string) {
    try { await cancelJob(id); await refresh(); if (selected?.id === id) selected = await getSession(id); }
    catch (e) { error = String(e); }
  }

  async function remove(id: string) {
    try { await deleteSession(id); if (selected?.id === id) selected = null; await refresh(); onSessionsChanged?.(); }
    catch (e) { error = String(e); }
  }

  async function confirmRemove(id: string) {
    if (deleteConfirmId === id) {
      clearTimeout(deleteTimer);
      deleteConfirmId = null;
      await remove(id);
    } else {
      deleteConfirmId = id;
      clearTimeout(deleteTimer);
      deleteTimer = setTimeout(() => (deleteConfirmId = null), 3000);
    }
  }

  export async function confirmDeleteAll() {
    if (recording || clearing) return;
    clearing = true;
    clearResult = '';
    try {
      const result = await clearAllSessions();
      if (!result) return;
      clearResult = `${result.deleted} deleted; ${result.skipped} skipped`;
      if (!result.skipped) selected = null;
      await refresh();
      const id = selected?.id;
      if (query && id) {
        const session = await getSession(id);
        if (selected?.id === id) selected = session;
      }
      onSessionsChanged?.();
    } catch (e) {
      await refresh();
      onSessionsChanged?.();
      error = String(e);
    } finally {
      clearing = false;
    }
  }

</script>

<div class="shell">
  <aside aria-label="Sidebar" data-tour="sessions">
    <input bind:this={searchField} type="search" aria-label="Search sessions" placeholder="Search sessions" bind:value={query} oninput={(e) => { query = e.currentTarget.value; void refresh(); }} />
    {#if error}<p role="alert">Could not update sidebar: {error} <button onclick={() => { void refresh(); }}>Retry</button></p>{/if}
    {#if clearResult}<p role="status">{clearResult}</p>{/if}
    <nav aria-label="Main navigation">
      <button onclick={(e) => onNavigate('record', e.currentTarget)} aria-current={view === 'record' ? 'page' : undefined} disabled={navigationLocked}>New Recording</button>
      <button data-tour="styles" onclick={(e) => onNavigate('styles', e.currentTarget)} aria-current={view === 'styles' ? 'page' : undefined} disabled={navigationLocked}>Note Styles</button>
    </nav>
    {#each groups as group (group.label)}
      {#if group.rows.length}
        <h2>{group.label}</h2>
        <ul>
          {#each group.rows as item (item.id)}
            {@const event = progress[item.id]}
            <li>
              <button class="session" onclick={(e) => open(item, e.currentTarget)} oncontextmenu={(e) => { e.preventDefault(); const menu = e.currentTarget.nextElementSibling; if (menu instanceof HTMLDetailsElement) menu.open = true; }} aria-current={view === 'history' && selected?.id === item.id ? 'page' : undefined} disabled={navigationLocked}>
                <strong>{item.title || new Date(item.createdAt).toLocaleString()}</strong>
                <time>{new Date(item.createdAt).toLocaleDateString('en-US')}</time>
                <span>{event?.message ?? item.status} {item.isSample ? 'Sample' : ''}</span>
                {#if item.interrupted && item.hasAudio && item.status === 'failed'}<span>Recover</span>{/if}
                {#if ['awaitingModel', 'summarizing'].includes(item.status)}<span>Writing…</span>{/if}
                {#if copiedId === item.id}<span aria-label="Copied">✓</span>{/if}
              </button>
              <details class="actions">
                <summary aria-label="Actions for {item.title || 'session'}">•••</summary>
                {#if item.interrupted && item.hasAudio && item.status === 'failed'}<button disabled={!allows($accessState)} onclick={() => recover(item.id)}>Recover</button>{/if}
                {#if item.status === 'awaitingModel'}<button onclick={() => cancel(item.id)}>Cancel</button>{/if}
                {#if item.status === 'finalizing' || item.status === 'summarizing'}<button onclick={() => cancel(item.id)}>Stop</button>{/if}
                {#if item.interrupted && !item.hasAudio}<span>Audio no longer available</span>{/if}
                {#if !['recording', 'finalizing', 'summarizing', 'awaitingModel'].includes(item.status)}
                  <button onclick={() => confirmRemove(item.id)}>{deleteConfirmId === item.id ? 'Confirm Delete' : 'Delete'}</button>
                {/if}
              </details>
            </li>
          {/each}
        </ul>
      {/if}
    {/each}
    <nav aria-label="More">
      <button data-tour="settings" onclick={() => openSettings().catch((e) => (error = String(e)))}>Settings</button>
      <button onclick={onTour}>Show tour</button>
      <TrialBadge onclick={onLicense} />
    </nav>
    {#if interrupted}<p role="status">{interrupted} interrupted recording{interrupted === 1 ? '' : 's'}</p>{/if}
  </aside>
  <div class="content">
    <div hidden={view !== 'history'}>
      {#if selected}
        <SummaryPanel bind:this={summaryPanel} session={selected} active={view === 'history'} management prompts={prompts} models={models}
          onUpdate={(s) => { selected = s; void refresh(); }} onCopied={markCopied}
          onDelete={['recording', 'finalizing'].includes(selected.status) ? undefined : () => remove(selected!.id)} />
      {:else}<p>Select a session or start a new recording.</p>{/if}
    </div>
    {@render children?.()}
  </div>
</div>

<style>
  .shell { display: grid; grid-template-columns: minmax(14rem, 17rem) minmax(0, 1fr); gap: 1rem; }
  aside { border-right: 1px solid #8884; padding: 1rem; min-height: 70vh; }
  aside input, aside nav button { width: 100%; }
  aside nav button { text-align: left; }
  aside h2 { font-size: .85rem; margin: 1.5rem 0 .3rem; }
  ul { padding: 0; margin: 0; list-style: none; }
  li { border-bottom: 1px solid #8884; padding: .4rem 0; }
  .session { display: grid; text-align: left; width: 100%; gap: .2rem; }
  time { font-size: .85em; }
  .actions { position: relative; }
  .actions summary { cursor: pointer; }
  .actions button { display: block; }
  @media (max-width: 700px) { .shell { grid-template-columns: minmax(10rem, 38%) minmax(0, 1fr); } }
</style>
