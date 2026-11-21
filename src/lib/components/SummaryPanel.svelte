<script lang="ts">
  import { onMount } from 'svelte';
  import { cancelJob, copyText, getPromptList, getSession, listSummaryModels, markNoteCopied, onSummaryDelta, openSettings, redoSummary, regenerateSummary, subscribeSummaryAvailability, undoSummary } from '../api';
  import type { PromptList } from '../types/PromptList';
  import type { SummaryModel } from '../types/SummaryModel';
  import type { SummaryAvailability } from '../types/SummaryAvailability';
  import type { Session } from '../types/Session';
  import SessionAudio from './SessionAudio.svelte';
  import SpeakersCard from './SpeakersCard.svelte';
  import StylePicker from './StylePicker.svelte';
  import SummaryUnavailable from './SummaryUnavailable.svelte';
  import LicenseGate from './LicenseGate.svelte';
  import { accessState, allows } from '../license';
  import { getCoachContext } from '../coach-context';
  import CoachEntry from './CoachEntry.svelte';
  import SessionCoach from './SessionCoach.svelte';

  const coach = getCoachContext();
  const availability = $derived(coach?.availability() ?? null);

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
    onCopied,
  }: { session: Session; active?: boolean; byline?: string; management?: boolean; prompts?: PromptList | null; models?: SummaryModel[]; onUpdate?: (session: Session) => void; onDelete?: () => void; onCopied?: (id: string) => void } = $props();
  let prompts = $state<PromptList | null>(null);
  let models = $state<SummaryModel[]>([]);
  let selectedPrompt = $state<string | null>(null);
  let preview = $state('');
  let copied = $state(false);
  let error = $state<string | null>(null);
  let busy = $state(false);
  let summaryAvailability = $state<SummaryAvailability>('downloading');
  let summaryAvailabilityError = $state(false);
  let summaryAvailabilityRetrying = $state(false);
  let retrySummaryAvailability: (() => void) | null = null;
  let confirmDelete = $state(false);
  let deleteTimer: ReturnType<typeof setTimeout> | undefined;
  let copyTimer: ReturnType<typeof setTimeout> | undefined;
  const sessionId = $derived(session.id);
  const savedSummary = $derived(session.summary);
  const working = $derived(['recording', 'finalizing', 'summarizing', 'awaitingModel'].includes(session.status));
  const provenance = $derived(byline || [
    session.promptId && prompts
      ? (prompts.prompts.find((p) => p.id === session.promptId)?.name ?? 'Deleted style')
      : null,
    session.modelId ? (models.find((m) => m.spec.id === session.modelId)?.spec.displayName ?? session.modelId) : null,
  ].filter(Boolean).join(' · '));

  $effect(() => {
    const styles = coach?.styles() ?? promptList;
    prompts = styles;
    models = modelList;
    selectedPrompt = styles?.prompts.some((p) => p.id === session.promptId)
      ? session.promptId
      : styles?.activeId ?? styles?.prompts.find((p) => p.showInStyleMenu)?.id ?? null;
  });

  $effect(() => {
    sessionId;
    preview = '';
    copied = false;
    error = null;
    busy = false;
    confirmDelete = false;
    clearTimeout(deleteTimer);
    clearTimeout(copyTimer);
  });

  $effect(() => {
    savedSummary;
    if (!working) preview = '';
  });

  onMount(() => {
    let disposed = false;
    let summaryAttempt = 0;
    let unlistenAvailability: (() => void) | undefined;
    const subscribe = async () => {
      const attempt = ++summaryAttempt;
      summaryAvailabilityRetrying = true;
      unlistenAvailability?.();
      unlistenAvailability = undefined;
      try {
        const remove = await subscribeSummaryAvailability((state) => {
          if (attempt !== summaryAttempt) return;
          summaryAvailability = state;
          summaryAvailabilityError = false;
        }, () => { if (!disposed && attempt === summaryAttempt) summaryAvailabilityError = true; });
        if (disposed || attempt !== summaryAttempt) remove(); else unlistenAvailability = remove;
      } catch {
        if (!disposed && attempt === summaryAttempt) summaryAvailabilityError = true;
      } finally {
        if (attempt === summaryAttempt) summaryAvailabilityRetrying = false;
      }
    };
    retrySummaryAvailability = subscribe;
    subscribe();
    if (management && !prompts) getPromptList().then((p) => (prompts = p)).catch(() => {});
    if (management && !models.length) listSummaryModels().then((m) => (models = m)).catch(() => {});
    const unlisten = management || coach ? onSummaryDelta((d) => {
      if (d.sessionId === sessionId && (management || savedSummary)) preview += d.text;
    }) : null;
    return () => {
      disposed = true;
      summaryAttempt++;
      unlistenAvailability?.();
      retrySummaryAvailability = null;
      unlisten?.then((fn) => fn());
      clearTimeout(deleteTimer);
      clearTimeout(copyTimer);
    };
  });

  export async function copy() {
    const { id, summary } = session;
    const text = summary;
    if (!text) return;
    copied = false;
    try {
      if (navigator.clipboard?.writeText) await navigator.clipboard.writeText(text);
      else await copyText(text);
      await markNoteCopied(id, summary);
      copied = true;
      if (summary) onCopied?.(id);
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

  async function redo() {
    try { const updated = await redoSummary(session.id); onUpdate?.(updated); }
    catch (e) { error = String(e); }
  }

  export function requestDelete() {
    confirmDelete = true;
    clearTimeout(deleteTimer);
    deleteTimer = setTimeout(() => (confirmDelete = false), 3000);
  }

  function deleteClick() {
    if (confirmDelete) { clearTimeout(deleteTimer); confirmDelete = false; onDelete?.(); return; }
    requestDelete();
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
  {#if management}<LicenseGate access={$accessState} onEnter={() => { void openSettings('License').catch((e) => (error = String(e))); }} showWarning={false} />{/if}
  {#if summaryAvailability === 'unsupported' || summaryAvailability === 'loadFailed'}
    <SummaryUnavailable />
  {:else if session.status === 'awaitingModel' && !summaryAvailabilityError}
    <p>Waiting for the summary model download…</p>
  {/if}
  {#if summaryAvailabilityError}
    <p role="alert">Couldn't check summary availability.</p>
    <button disabled={summaryAvailabilityRetrying} onclick={() => retrySummaryAvailability?.()}>Check again</button>
  {/if}
  {#if session.status === 'failed'}
    <p role="alert">{session.error}</p>
  {:else if session.status === 'cancelled'}
    <p>Stopped.</p>
  {/if}
  {#if management}
    {#if provenance}<p>{provenance}</p>{/if}
  {/if}
  {#if management || session.summary}
    <button onclick={copy} disabled={!session.summary}>{copied ? 'Copied' : 'Copy Note'}</button>
  {/if}
  {#if management || (session.summary && coach)}
    {#if prompts}
      <StylePicker prompts={prompts.prompts} selectedId={selectedPrompt} disabled={!allows($accessState)} onSelect={(id) => (selectedPrompt = id)} coachEntry={coach ? newStyle : undefined} />
    {/if}
    {#snippet newStyle()}
      {#if coach}<CoachEntry {availability} onOpen={(opener) => coach.open({}, opener)} showRetry />{/if}
    {/snippet}
    {#if coach && active && session.summary && !session.isSample && coach.firstNoteTip(session.id)}
      <aside class="tip">
        <strong>Want notes that look like yours?</strong>
        <p>Give the coach an example of how you write so it can make a style for your future notes.</p>
        <CoachEntry {availability} onOpen={(opener) => coach.open({}, opener, coach.dismissTip)} showRetry />
        <button onclick={coach.dismissTip}>Dismiss tip</button>
      </aside>
    {/if}
    <button onclick={regenerate} disabled={!allows($accessState) || summaryAvailability !== 'ready' || working || busy || !session.transcript || !selectedPrompt}>Regenerate</button>
    {#if session.status === 'summarizing'}<button onclick={stop}>Stop</button>{/if}
    {#if session.status === 'awaitingModel'}<button onclick={stop}>Cancel</button>{/if}
    {#if session.previousSummary}<button onclick={undo} disabled={!allows($accessState)}>Undo</button>{/if}
    {#if session.canRedo}<button onclick={redo} disabled={!allows($accessState)}>Redo</button>{/if}
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
  {#if active && session.status === 'done' && session.summary}
    {#key session.id}<SessionCoach {session} {availability} {onUpdate} />{/key}
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
  .tip { border: 1px solid #8885; border-radius: 6px; padding: 10px; margin-block: 8px; }
  .tip p { margin-block: 5px; }
</style>
