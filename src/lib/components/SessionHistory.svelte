<script lang="ts">
  import { onMount, tick, type Snippet } from 'svelte';
  import { cancelJob, clearAllSessions, countSessions, deleteSession, getPromptList, getSession, listSessions, listSummaryModels, onSessionProgress, openSettings, recoverSession, searchSessions, setShellMenuState } from '../api';
  import { navigationDisabled, type AppView } from '../navigation';
  import type { Session } from '../types/Session';
  import type { SessionListItem } from '../types/SessionListItem';
  import type { SessionProgress } from '../types/SessionProgress';
  import type { NoteTab } from './note-tabs';
  import SummaryPanel from './SummaryPanel.svelte';
  import TrialBadge from './TrialBadge.svelte';
  import { accessState, allows } from '../license';

  let { view = 'history', navigationLocked = false, recording = false, recoverable = false, onNavigate = () => {}, onLicense = () => {}, onSessionsChanged, children }: {
    view?: AppView; navigationLocked?: boolean; recording?: boolean; recoverable?: boolean; onNavigate?: (view: AppView, opener?: HTMLElement, action?: () => void) => void;
    onLicense?: () => void; onSessionsChanged?: () => void; children?: Snippet;
  } = $props();
  let sessions = $state<SessionListItem[]>([]);
  let hasSessions = $state(false);
  let selected = $state<Session | null>(null);
  let progress = $state<Record<string, SessionProgress>>({});
  let error = $state<string | null>(null);
  let clearResult = $state('');
  let clearing = $state(false);
  let query = $state('');
  let searchField: HTMLInputElement;
  let sidebar: HTMLElement;
  let sidebarVisible = $state(true);
  let request = 0;
  let deleteConfirmId = $state<string | null>(null);
  let summaryPanel = $state<{ copy: () => Promise<void>; showTab: (tab: NoteTab) => void; undo: () => Promise<boolean>; redo: () => Promise<boolean> } | undefined>();
  let confirmingDelete = $state<string | null>(null);
  let recovering = $state(false);
  let deleteTimer: ReturnType<typeof setTimeout> | undefined;
  let prompts = $state<Awaited<ReturnType<typeof getPromptList>> | null>(null);
  let models = $state<Awaited<ReturnType<typeof listSummaryModels>>>([]);
  const working = (status: string) => ['recording', 'finalizing', 'summarizing', 'awaitingModel'].includes(status);
  // R16: an interrupted recording whose saved chunks can still be processed.
  const needsRecovery = (s: Session) => s.status === 'failed' && s.error === 'interrupted' && s.hasAudio;
  $effect(() => {
    // Refresh native availability after a list change, even when search hides every row.
    sessions; $accessState;
    void setShellMenuState({ selected: view === 'history' && !!selected && !['recording', 'finalizing'].includes(selected.status), hasNote: view === 'history' && !!selected?.summary && !working(selected.status), recording, recoverable, locked: navigationLocked }).catch(() => {});
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
    const filter = query;
    try {
      const result = filter ? await searchSessions(filter) : await listSessions();
      if (current !== request) return;
      sessions = result;
      error = null;
      if (!filter && selected && !result.some((s) => s.id === selected?.id)) selected = null;
      if (filter && !result.length) {
        const count = await countSessions();
        if (current === request) hasSessions = count > 0;
      } else {
        hasSessions = result.length > 0;
      }
    } catch (e) { if (current === request) error = String(e); }
  }

  function open(item: SessionListItem, opener: HTMLElement) {
    onNavigate('history', opener, () => openSession(item.id));
  }

  /** Shows a session's page, such as the one Stop just returned (R10). */
  export function openSession(id: string) {
    confirmingDelete = null;
    void getSession(id).then((session) => { if (session) selected = session; void refresh(); }).catch((e) => (error = String(e)));
  }

  export async function focusSearch() {
    sidebarVisible = true;
    await tick();
    searchField?.focus();
    searchField?.select();
  }
  export function toggleSidebar() { sidebarVisible = !sidebarVisible; }
  export function moveSession(direction: -1 | 1) {
    if (navigationLocked || !sessions.length) return;
    const index = sessions.findIndex((item) => item.id === selected?.id);
    const next = sessions[Math.max(0, Math.min(sessions.length - 1, index + direction))];
    const button = [...sidebar.querySelectorAll<HTMLButtonElement>('button.session')].find((row) => row.dataset.sessionId === next.id);
    if (button) open(next, button);
  }
  function sidebarKeydown(event: KeyboardEvent) {
    const rows = [...sidebar.querySelectorAll<HTMLButtonElement>('button.session:not(:disabled)')];
    const index = rows.indexOf(event.target as HTMLButtonElement);
    if (index < 0 || event.metaKey || event.ctrlKey || event.altKey || event.shiftKey || (event.key !== 'ArrowUp' && event.key !== 'ArrowDown')) return;
    event.preventDefault();
    rows[Math.max(0, Math.min(rows.length - 1, index + (event.key === 'ArrowDown' ? 1 : -1)))]?.focus();
  }
  export function confirmDeleteSelected() { if (view === 'history' && selected && !['recording', 'finalizing'].includes(selected.status)) confirmingDelete = selected.id; }
  export async function copySelectedNote() { if (view === 'history' && selected?.summary && !working(selected.status)) await summaryPanel?.copy(); }
  export function showTab(tab: NoteTab) { if (view === 'history') summaryPanel?.showTab(tab); }
  /** Note-level Undo/Redo for the Edit menu; false when there is nothing to do here. */
  export async function undoNote() { return view === 'history' && !!summaryPanel && await summaryPanel.undo(); }
  export async function redoNote() { return view === 'history' && !!summaryPanel && await summaryPanel.redo(); }
  /** Recovers the open interrupted session, or opens the first one so its page explains recovery. */
  export async function recoverFirst() {
    try {
      if (selected && needsRecovery(selected)) { await recover(selected.id); return; }
      const first = (await listSessions()).find((s) => s.interrupted && s.hasAudio && s.status === 'failed');
      if (first) openSession(first.id);
    } catch (e) { error = String(e); }
  }
  export function refreshSessions() { void refresh(); }

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
    window.addEventListener('keydown', sidebarKeydown);
    void refresh();
    getPromptList().then((p) => (prompts = p)).catch(() => {});
    listSummaryModels().then((m) => (models = m)).catch(() => {});
    const unlisten = onSessionProgress((p) => { void applyProgress(p).catch((e) => (error = String(e))); });
    return () => { window.removeEventListener('keydown', sidebarKeydown); unlisten.then((fn) => fn()); clearTimeout(deleteTimer); };
  });

  async function recover(id: string) {
    error = null;
    recovering = true;
    try { await recoverSession(id); }
    catch (e) { error = String(e); }
    finally {
      recovering = false;
      await refresh();
      if (selected?.id === id) selected = await getSession(id).catch(() => selected);
    }
  }

  async function cancel(id: string) {
    try { await cancelJob(id); await refresh(); if (selected?.id === id) selected = await getSession(id); }
    catch (e) { error = String(e); }
  }

  async function remove(id: string) {
    confirmingDelete = null;
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

<div class="shell" class:sidebar-hidden={!sidebarVisible}>
  <aside bind:this={sidebar} aria-label="Sidebar" data-tour="sessions" hidden={!sidebarVisible}>
    <div class="sidebar">
      <input bind:this={searchField} type="search" aria-label="Search sessions" placeholder="Search sessions" bind:value={query} oninput={(e) => { query = e.currentTarget.value; void refresh(); }} />
      {#if error}<p role="alert">Could not update sidebar: {error} <button onclick={() => { void refresh(); }}>Retry</button></p>{/if}
      {#if clearResult}<p role="status">{clearResult}</p>{/if}
      <nav aria-label="Main navigation">
        <button onclick={(e) => onNavigate('record', e.currentTarget)} aria-current={view === 'record' ? 'page' : undefined} disabled={navigationDisabled(navigationLocked, 'record', view)}>New Recording</button>
        <button onclick={(e) => onNavigate('styles', e.currentTarget)} aria-current={view === 'styles' ? 'page' : undefined}>Note Styles</button>
      </nav>
      <div class="sessions">
        {#each groups as group (group.label)}
          {#if group.rows.length}
            <h2>{group.label}</h2>
            <ul>
              {#each group.rows as item (item.id)}
                {@const event = progress[item.id]}
                <li>
                  <button class="session" data-session-id={item.id} onclick={(e) => open(item, e.currentTarget)} oncontextmenu={(e) => { e.preventDefault(); const menu = e.currentTarget.nextElementSibling; if (menu instanceof HTMLDetailsElement) menu.open = true; }} aria-current={view === 'history' && selected?.id === item.id ? 'page' : undefined} disabled={navigationLocked}>
                    <strong>{item.title || new Date(item.createdAt).toLocaleString()}</strong>
                    <time>{new Date(item.createdAt).toLocaleDateString('en-US')}</time>
                    <span>{event?.message ?? item.status} {item.isSample ? 'Sample' : ''}</span>
                    {#if item.interrupted && item.hasAudio && item.status === 'failed'}<span>Recover</span>{/if}
                    {#if ['finalizing', 'awaitingModel', 'summarizing'].includes(item.status)}<span>Writing…</span>{/if}
                    {#if item.copiedAt !== null}<span aria-label="Copied">✓</span>{/if}
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
      </div>
      <nav aria-label="More">
        <button onclick={() => openSettings().catch((e) => (error = String(e)))}>Settings</button>
        <TrialBadge onclick={onLicense} />
      </nav>
      <button class="delete-all" disabled={!hasSessions || recording || clearing} onclick={confirmDeleteAll}>Delete All…</button>
    </div>
  </aside>
  <div class="content">
    <div hidden={view !== 'history'}>
      {#if selected && confirmingDelete === selected.id}
        {@const id = selected.id}
        <div role="alertdialog" aria-label="Delete session">
          <p>Delete “{selected.title}”? Its note, transcript and audio are removed from this Mac.</p>
          <button onclick={() => remove(id)}>Delete Session</button>
          <button onclick={() => (confirmingDelete = null)}>Cancel</button>
        </div>
      {/if}
      {#if selected && needsRecovery(selected)}
        {@const id = selected.id}
        <article aria-label="Recover recording">
          <h3>{selected.title}</h3>
          <p>This recording stopped before it finished. Its audio is saved on this Mac, so Scribular can recover it and write the transcript and note.</p>
          <button disabled={recovering || !allows($accessState)} onclick={() => recover(id)}>Recover Recording</button>
          <button disabled={recovering} onclick={confirmDeleteSelected}>Delete</button>
        </article>
      {:else if selected}
        <SummaryPanel bind:this={summaryPanel} session={selected} active={view === 'history'} management prompts={prompts} models={models}
          progress={progress[selected.id]}
          onUpdate={(s) => { selected = s; void refresh(); }} onCopied={() => { void refresh(); }} />
      {:else}<p>Select a session or start a new recording.</p>{/if}
    </div>
    {@render children?.()}
  </div>
</div>

<style>
  .shell { display: grid; grid-template-columns: minmax(14rem, 17rem) minmax(0, 1fr); gap: 1rem; }
  .shell.sidebar-hidden { grid-template-columns: minmax(0, 1fr); }
  aside { border-right: 1px solid #8884; padding: 1rem; }
  .sidebar { display: flex; flex-direction: column; height: 70vh; }
  .sessions { flex: 1; overflow-y: auto; }
  .delete-all { align-self: start; margin-top: .75rem; }
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
