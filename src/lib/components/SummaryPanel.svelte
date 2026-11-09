<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import { cancelJob, copyText, getPromptList, getSession, listSummaryModels, onSummaryDelta, regenerateSummary, subscribeSummaryAvailability, undoSummary } from '../api';
  import type { PromptList } from '../types/PromptList';
  import type { SummaryModel } from '../types/SummaryModel';
  import type { SummaryAvailability } from '../types/SummaryAvailability';
  import type { Session } from '../types/Session';
  import SessionAudio from './SessionAudio.svelte';
  import SpeakersCard from './SpeakersCard.svelte';
  import StylePicker from './StylePicker.svelte';
  import SummaryUnavailable from './SummaryUnavailable.svelte';

  // `active` is false while the record view is hidden behind Settings.
  let {
    session,
    active = true,
    byline = '',
    management = false,
    prompts: promptList = null,
    models: modelList = [],
    onUpdate,
    onDelete,
  }: { session: Session; active?: boolean; byline?: string; management?: boolean; prompts?: PromptList | null; models?: SummaryModel[]; onUpdate?: (session: Session) => void; onDelete?: () => void } = $props();
  let prompts = $state<PromptList | null>(null);
  let models = $state<SummaryModel[]>([]);
  let selectedPrompt = $state<string | null>(null);
  let preview = $state('');
  let copied = $state(false);
  let error = $state<string | null>(null);
  let busy = $state(false);
  let summaryAvailability = $state<SummaryAvailability>('downloading');
  let summaryAvailabilityError = $state(false);
  let retrySummaryAvailability: (() => void) | null = null;
  let confirmDelete = $state(false);
  let deleteTimer: ReturnType<typeof setTimeout> | undefined;
  let copyTimer: ReturnType<typeof setTimeout> | undefined;
  const working = $derived(['recording', 'finalizing', 'summarizing', 'awaitingModel'].includes(session.status));
  const provenance = $derived(byline || [
    session.promptId && prompts
      ? (prompts.prompts.find((p) => p.id === session.promptId)?.name ?? 'Deleted style')
      : null,
    session.modelId ? (models.find((m) => m.spec.id === session.modelId)?.spec.displayName ?? session.modelId) : null,
  ].filter(Boolean).join(' · '));

  $effect(() => {
    prompts = promptList;
    models = modelList;
    selectedPrompt = promptList?.prompts.some((p) => p.id === session.promptId)
      ? session.promptId
      : promptList?.activeId ?? promptList?.prompts[0]?.id ?? null;
  });

  $effect(() => {
    const id = session.id;
    untrack(() => {
      preview = '';
      copied = false;
      error = null;
      busy = false;
      confirmDelete = false;
      clearTimeout(deleteTimer);
      clearTimeout(copyTimer);
    });
  });

  onMount(() => {
    let disposed = false;
    let unlistenAvailability: (() => void) | undefined;
    const subscribe = () => {
      unlistenAvailability?.();
      unlistenAvailability = undefined;
      void subscribeSummaryAvailability((state) => {
        summaryAvailability = state;
        summaryAvailabilityError = false;
      }, () => { summaryAvailabilityError = true; }).then((remove) => {
        if (disposed) remove(); else unlistenAvailability = remove;
      }).catch(() => { summaryAvailabilityError = true; });
    };
    retrySummaryAvailability = subscribe;
    subscribe();
    if (management && !prompts) getPromptList().then((p) => (prompts = p)).catch(() => {});
    if (management && !models.length) listSummaryModels().then((m) => (models = m)).catch(() => {});
    const unlisten = management ? onSummaryDelta((d) => { if (d.sessionId === session.id) preview += d.text; }) : null;
    return () => {
      disposed = true;
      unlistenAvailability?.();
      retrySummaryAvailability = null;
      unlisten?.then((fn) => fn());
      clearTimeout(deleteTimer);
      clearTimeout(copyTimer);
    };
  });

  async function copy() {
    const text = session.summary || session.transcript;
    try {
      if (navigator.clipboard?.writeText) await navigator.clipboard.writeText(text);
      else await copyText(text);
      copied = true;
      clearTimeout(copyTimer);
      copyTimer = setTimeout(() => (copied = false), 2000);
    } catch (e) { error = String(e); }
  }

  async function regenerate() {
    if (!selectedPrompt) return;
    error = null; preview = ''; busy = true;
    try { await regenerateSummary(session.id, selectedPrompt); }
    catch (e) { error = String(e); }
    finally { busy = false; }
  }

  async function undo() {
    try { const updated = await undoSummary(session.id); preview = ''; onUpdate?.(updated); }
    catch (e) { error = String(e); }
  }

  function deleteClick() {
    if (confirmDelete) { clearTimeout(deleteTimer); confirmDelete = false; onDelete?.(); return; }
    confirmDelete = true;
    deleteTimer = setTimeout(() => (confirmDelete = false), 3000);
  }

  async function stop() {
    try {
      await cancelJob(session.id);
      preview = '';
      const updated = await getSession(session.id);
      if (updated) onUpdate?.(updated);
    }
    catch (e) { error = String(e); }
  }
</script>

<article>
  <h3>{session.title}</h3>
  {#if summaryAvailability === 'unsupported' || summaryAvailability === 'loadFailed'}
    <SummaryUnavailable />
  {:else if (summaryAvailability === 'downloading' || session.status === 'awaitingModel') && !summaryAvailabilityError}
    <p>Waiting for the summary model download…</p>
  {/if}
  {#if summaryAvailabilityError}
    <p role="alert">Couldn't check summary availability.</p>
    <button onclick={() => retrySummaryAvailability?.()}>Check again</button>
  {/if}
  {#if session.status === 'failed'}
    <p role="alert">{session.error}</p>
  {:else if session.status === 'cancelled'}
    <p>Stopped.</p>
  {/if}
  {#if management}
    {#if provenance}<p>{provenance}</p>{/if}
    <button onclick={copy} disabled={!session.summary && !session.transcript}>{copied ? 'Copied' : 'Copy'}</button>
    {#if prompts}
      <StylePicker prompts={prompts.prompts} selectedId={selectedPrompt} includeId={session.promptId} onSelect={(id) => (selectedPrompt = id)} />
    {/if}
    <button onclick={regenerate} disabled={summaryAvailability !== 'ready' || working || busy || !session.transcript || !selectedPrompt}>Regenerate</button>
    {#if session.status === 'summarizing'}<button onclick={stop}>Stop</button>{/if}
    {#if session.status === 'awaitingModel'}<button onclick={stop}>Cancel</button>{/if}
    {#if session.previousSummary}<button onclick={undo}>Undo</button>{/if}
    {#if onDelete}
      <button onclick={deleteClick}>{confirmDelete ? 'Confirm' : 'Delete'}</button>
    {/if}
    {#if preview}<h4>New summary preview</h4><pre>{preview}</pre>{/if}
    {#if error}<p role="alert">{error}</p>{/if}
  {/if}
  {#if session.summary}
    <h4>Summary{#if provenance && !management} · {provenance}{/if}</h4>
    <pre>{session.summary}</pre>
  {/if}
  <SpeakersCard {session} {onUpdate} />
  <SessionAudio {session} {active} />
  {#if session.transcript && session.diarizationState !== 'confirmed'}
    <h4>Transcript</h4>
    <pre>{session.transcript}</pre>
  {/if}
</article>

<style>
  pre { white-space: pre-wrap; }
</style>
