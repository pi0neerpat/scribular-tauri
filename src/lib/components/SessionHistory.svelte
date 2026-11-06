<script lang="ts">
  import { onMount } from 'svelte';
  import { cancelJob, clearAllSessions, deleteSession, getPromptList, getSession, listSessions, listSummaryModels, onSessionProgress, recoverSession } from '../api';
  import type { Session } from '../types/Session';
  import type { SessionListItem } from '../types/SessionListItem';
  import type { SessionProgress } from '../types/SessionProgress';
  import SummaryPanel from './SummaryPanel.svelte';

  let { active = true, onSessionsChanged }: { active?: boolean; onSessionsChanged?: () => void } = $props();
  let sessions = $state<SessionListItem[]>([]);
  let selected = $state<Session | null>(null);
  let progress = $state<Record<string, SessionProgress>>({});
  let error = $state<string | null>(null);
  let confirmClear = $state(false);
  let clearResult = $state('');
  let clearing = $state(false);
  let deleteConfirmId = $state<string | null>(null);
  let deleteTimer: ReturnType<typeof setTimeout> | undefined;
  let prompts = $state<Awaited<ReturnType<typeof getPromptList>> | null>(null);
  let models = $state<Awaited<ReturnType<typeof listSummaryModels>>>([]);
  let interrupted = $derived(sessions.filter((s) => s.interrupted && s.hasAudio && s.status === 'failed').length);

  async function refresh() {
    sessions = await listSessions();
    if (selected && !sessions.some((s) => s.id === selected?.id)) selected = null;
  }

  async function open(item: SessionListItem) {
    try { selected = await getSession(item.id); }
    catch (e) { error = String(e); }
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
    refresh().catch((e) => (error = String(e)));
    getPromptList().then((p) => (prompts = p)).catch(() => {});
    listSummaryModels().then((m) => (models = m)).catch(() => {});
    const unlisten = onSessionProgress((p) => { void applyProgress(p).catch((e) => (error = String(e))); });
    return () => { unlisten.then((fn) => fn()); clearTimeout(deleteTimer); };
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
    try { await deleteSession(id); await refresh(); onSessionsChanged?.(); }
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

  async function clear() {
    if (clearing) return;
    clearing = true;
    try {
      const result = await clearAllSessions();
      clearResult = `${result.deleted} deleted${result.skipped ? `; ${result.skipped} session${result.skipped === 1 ? '' : 's'} not deleted` : ''}`;
      confirmClear = false;
      await refresh();
      onSessionsChanged?.();
    } catch (e) { error = String(e); } finally { clearing = false; }
  }
</script>

<section aria-label="Session history" data-tour="sessions">
  <h2>History</h2>
  {#if interrupted}
    <p role="status">{interrupted} interrupted recording{interrupted === 1 ? '' : 's'} — recover?</p>
  {/if}
  <p>{sessions.length} {sessions.length === 1 ? 'session' : 'sessions'}</p>
  {#if error}<p role="alert">{error}</p>{/if}
  {#if clearResult}<p role="status">{clearResult}</p>{/if}
  {#if confirmClear}
    <p>Delete all sessions? This cannot be undone.</p>
    <button onclick={clear} disabled={clearing}>Confirm delete all</button>
    <button onclick={() => (confirmClear = false)}>Cancel</button>
  {:else}
    <button onclick={() => (confirmClear = true)} disabled={!sessions.length}>Delete all sessions</button>
  {/if}
  <div class="history">
    <ul>
      {#each sessions as item (item.id)}
        {@const event = progress[item.id]}
        <li>
          <button class="session" onclick={() => open(item)} aria-current={selected?.id === item.id ? 'true' : undefined}>
            <strong>{item.title || new Date(item.createdAt).toLocaleString()}</strong>
            <time>{new Date(item.createdAt).toLocaleString()}</time>
            <span>{event?.message ?? item.status}</span>
            <span>{item.status === 'awaitingModel' ? 'Waiting for the summary model download…' : item.status === 'summarizing' ? 'Summarizing…' : item.snippet}</span>
          </button>
          {#if item.interrupted && item.hasAudio && item.status === 'failed'}
            <button onclick={() => recover(item.id)}>Recover</button>
          {:else if item.interrupted && !item.hasAudio}
            <span>Audio no longer available</span>
            <button onclick={() => confirmRemove(item.id)}>{deleteConfirmId === item.id ? 'Confirm' : 'Delete'}</button>
          {:else if item.status === 'failed' || item.status === 'cancelled' || item.status === 'done'}
            <button onclick={() => confirmRemove(item.id)}>{deleteConfirmId === item.id ? 'Confirm' : 'Delete'}</button>
          {:else if item.status === 'awaitingModel'}
            <button onclick={() => cancel(item.id)}>Cancel</button>
          {:else if item.status === 'finalizing'}
            <button onclick={() => cancel(item.id)}>Stop</button>
          {:else if item.status === 'summarizing'}
            <button onclick={() => cancel(item.id)}>Stop</button>
          {/if}
        </li>
      {/each}
    </ul>
    {#if selected}
      <SummaryPanel session={selected} {active} management prompts={prompts} models={models}
        onUpdate={(s) => { selected = s; void refresh(); }}
        onDelete={['recording', 'finalizing'].includes(selected.status) ? undefined : () => remove(selected!.id)} />
    {/if}
  </div>
</section>

<style>
  .history { display: grid; grid-template-columns: minmax(16rem, 1fr) minmax(20rem, 2fr); gap: 1rem; }
  ul { padding: 0; list-style: none; }
  li { border-bottom: 1px solid #8884; padding: .5rem 0; }
  .session { display: grid; text-align: left; width: 100%; gap: .25rem; }
  time { font-size: .85em; }
  @media (max-width: 700px) { .history { grid-template-columns: 1fr; } }
</style>
