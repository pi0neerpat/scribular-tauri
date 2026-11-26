<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import { cancelJob, copyText, getPromptList, getSession, listSummaryModels, markNoteCopied, onSummaryDelta, openSettings, redoSummary, regenerateSummary, retrySummaryModel, subscribeSummaryAvailability, undoSummary, updateSession } from '../api';
  import type { PromptList } from '../types/PromptList';
  import type { SummaryModel } from '../types/SummaryModel';
  import type { SummaryAvailability } from '../types/SummaryAvailability';
  import type { Session } from '../types/Session';
  import type { SessionProgress } from '../types/SessionProgress';
  import type { NoteTab } from './note-tabs';
  import SessionAudio from './SessionAudio.svelte';
  import SpeakersCard from './SpeakersCard.svelte';
  import StylePicker from './StylePicker.svelte';
  import SummaryUnavailable from './SummaryUnavailable.svelte';
  import LicenseGate from './LicenseGate.svelte';
  import { accessState, allows } from '../license';
  import { getCoachContext } from '../coach-context';
  import { audioUnavailable } from './session-audio';
  import CoachEntry from './CoachEntry.svelte';
  import SessionCoach from './SessionCoach.svelte';

  const coach = getCoachContext();
  const availability = $derived(coach?.availability() ?? null);

  // `active` is false while the session page is hidden behind another view.
  let {
    session,
    active = true,
    byline = '',
    management = false,
    prompts: promptList = null,
    models: modelList = [],
    progress = undefined,
    onUpdate,
    onCopied,
  }: { session: Session; active?: boolean; byline?: string; management?: boolean; prompts?: PromptList | null; models?: SummaryModel[]; progress?: SessionProgress; onUpdate?: (session: Session) => void; onCopied?: (id: string) => void } = $props();
  let prompts = $state<PromptList | null>(null);
  let models = $state<SummaryModel[]>([]);
  let selectedPrompt = $state<string | null>(null);
  let preview = $state('');
  let copied = $state(false);
  let transcriptCopied = $state(false);
  let error = $state<string | null>(null);
  let busy = $state(false);
  let tab = $state<NoteTab>('note');
  // Seeded here, not only in the effect, so the first paint (and SSR) shows the note.
  let draft = $state(untrack(() => session.summary));
  let draftFor = untrack(() => session.id);
  let lastSaved = untrack(() => session.summary);
  // R12: the Undo line shows after a rewrite or Coach acceptance until edit, copy or navigation.
  let undoLine = $state(false);
  /** The note a pending rewrite replaces; null when no rewrite is pending. */
  let rewriteFrom = $state<string | null>(null);
  let summaryAvailability = $state<SummaryAvailability>('downloading');
  let summaryAvailabilityError = $state(false);
  let summaryAvailabilityRetrying = $state(false);
  let modelRetrying = $state(false);
  let retrySummaryAvailability: (() => void) | null = null;
  let copyTimer: ReturnType<typeof setTimeout> | undefined;
  const sessionId = $derived(session.id);
  const savedSummary = $derived(session.summary);
  const working = $derived(['recording', 'finalizing', 'summarizing', 'awaitingModel'].includes(session.status));
  const writable = $derived(allows($accessState));
  const unavailable = $derived(summaryAvailability === 'unsupported' || summaryAvailability === 'loadFailed');
  const dirty = $derived(draft !== savedSummary);
  const noAudio = $derived(audioUnavailable(session));
  const styleName = $derived(session.promptId && prompts
    ? (prompts.prompts.find((p) => p.id === session.promptId)?.name ?? 'Deleted style')
    : null);
  const provenance = $derived(byline || [
    styleName,
    session.modelId ? (models.find((m) => m.spec.id === session.modelId)?.spec.displayName ?? session.modelId) : null,
  ].filter(Boolean).join(' · '));
  const speakerSummary = $derived(session.diarizationState === 'confirmed' && session.speakerMap
    ? [...new Set(Object.values(session.speakerMap))].join(', ') : '');
  const when = $derived([
    new Date(session.createdAt).toLocaleString('en-US', { month: 'short', day: 'numeric', hour: 'numeric', minute: '2-digit' }),
    session.durationMs ? `${Math.floor(session.durationMs / 60000)}:${String(Math.floor(session.durationMs / 1000) % 60).padStart(2, '0')}` : null,
  ].filter(Boolean).join(' · '));

  $effect(() => {
    const styles = coach?.styles() ?? promptList;
    if (styles) prompts = styles;
    if (modelList.length) models = modelList;
    selectedPrompt = styles?.prompts.some((p) => p.id === session.promptId)
      ? session.promptId
      : styles?.activeId ?? styles?.prompts.find((p) => p.showInStyleMenu)?.id ?? null;
  });

  // Navigation: a new session opens on Note; unsaved edits of the old one are saved first.
  $effect(() => {
    const id = sessionId;
    untrack(() => {
      if (draftFor && draftFor !== id && draft !== lastSaved) void save(draftFor, draft);
      draftFor = id;
      draft = session.summary;
      lastSaved = session.summary;
      tab = 'note';
      preview = '';
      copied = false;
      transcriptCopied = false;
      undoLine = false;
      rewriteFrom = null;
      error = null;
      busy = false;
      clearTimeout(copyTimer);
    });
  });

  // A saved note that changed underneath (rewrite, Undo, Coach) replaces a clean draft.
  $effect(() => {
    const summary = savedSummary;
    untrack(() => {
      if (draft === lastSaved) draft = summary;
      lastSaved = summary;
      if (!working) preview = '';
    });
  });

  $effect(() => {
    // The job may not have started when regenerateSummary resolves (status still 'done'), so wait for the saved note to change.
    if (rewriteFrom === null || working) return;
    if (session.status !== 'done') rewriteFrom = null;
    else if (session.summary !== rewriteFrom) { rewriteFrom = null; undoLine = Boolean(session.previousSummary); }
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
    const unlisten = onSummaryDelta((d) => { if (d.sessionId === sessionId) preview += d.text; });
    return () => {
      disposed = true;
      summaryAttempt++;
      unlistenAvailability?.();
      retrySummaryAvailability = null;
      unlisten.then((fn) => fn());
      clearTimeout(copyTimer);
    };
  });

  // The in-flight save, so history actions run after it instead of racing it.
  let saving: Promise<boolean> | null = null;
  function save(id: string, summary: string): Promise<boolean> {
    const pending = persist(id, summary);
    saving = pending;
    void pending.finally(() => { if (saving === pending) saving = null; });
    return pending;
  }

  /** Waits for any in-flight save, then saves an unsaved draft; false when a save failed. */
  async function flushDraft(): Promise<boolean> {
    if (saving && !(await saving)) return false;
    return !dirty || save(session.id, draft);
  }

  async function persist(id: string, summary: string): Promise<boolean> {
    try {
      const updated = await updateSession(id, { summary });
      if (id === sessionId) { lastSaved = updated.summary; undoLine = false; onUpdate?.(updated); }
      return true;
    } catch (e) { error = String(e); return false; }
  }

  async function commitEdit() {
    if (dirty && writable && !working) await save(session.id, draft);
  }

  async function writeClipboard(text: string) {
    if (navigator.clipboard?.writeText) await navigator.clipboard.writeText(text);
    else await copyText(text);
  }

  /** Copies the final note; copied_at is set only after the clipboard write succeeds (R13). */
  export async function copy() {
    if (working || !session.summary) return;
    if (writable && !working && !(await flushDraft())) return;
    const { id, summary } = session;
    copied = false;
    try {
      await writeClipboard(summary);
      await markNoteCopied(id, summary);
      copied = true;
      undoLine = false;
      onCopied?.(id);
      clearTimeout(copyTimer);
      copyTimer = setTimeout(() => (copied = false), 2000);
    } catch (e) { error = String(e); }
  }

  /** Copy Transcript never marks the note copied (R13). */
  async function copyTranscript() {
    try {
      await writeClipboard(session.transcript);
      transcriptCopied = true;
      clearTimeout(copyTimer);
      copyTimer = setTimeout(() => (transcriptCopied = false), 2000);
    } catch (e) { error = String(e); }
  }

  /** Picking a style rewrites at once, saving an unsaved edit first (R12). */
  async function rewrite(promptId: string) {
    selectedPrompt = promptId;
    if (!canRewrite) return;
    if (dirty && !(await save(session.id, draft))) return;
    error = null; preview = ''; busy = true; undoLine = false;
    const from = session.summary;
    try { await regenerateSummary(session.id, promptId); rewriteFrom = from; }
    catch (e) { error = String(e); }
    finally { busy = false; }
  }
  const canRewrite = $derived(writable && summaryAvailability === 'ready' && !working && !busy && !!session.transcript);

  export function showTab(next: NoteTab) {
    if (next === 'audio' && noAudio) return;
    tab = next;
  }

  /** Note Undo: true when a step was undone. */
  export async function undo(): Promise<boolean> {
    if (!session.previousSummary || !writable || working) return false;
    if (!(await flushDraft()) || !session.previousSummary) return false;
    try { const updated = await undoSummary(session.id); preview = ''; undoLine = false; onUpdate?.(updated); return true; }
    catch (e) { error = String(e); return false; }
  }

  export async function redo(): Promise<boolean> {
    if (!session.canRedo || !writable || working) return false;
    if (!(await flushDraft()) || !session.canRedo) return false;
    try { const updated = await redoSummary(session.id); onUpdate?.(updated); return true; }
    catch (e) { error = String(e); return false; }
  }

  async function retryModel() {
    modelRetrying = true;
    try { await retrySummaryModel(); } catch (e) { error = String(e); }
    finally { modelRetrying = false; }
  }

  async function stop() {
    try {
      await cancelJob(session.id);
      preview = '';
      rewriteFrom = null;
      const updated = await getSession(session.id);
      if (updated) onUpdate?.(updated);
    }
    catch (e) { error = String(e); }
  }

  function coachAccepted(updated: Session) {
    onUpdate?.(updated);
    undoLine = Boolean(updated.previousSummary);
  }

  const tabs: [NoteTab, string][] = [['note', 'Note'], ['transcript', 'Transcript'], ['audio', 'Audio']];
</script>

<article>
  <header>
    <h3>{session.title}</h3>
    <p>{when}{#if speakerSummary} · {speakerSummary}{/if}{#if provenance} · {provenance}{/if}</p>
    {#if management}<LicenseGate access={$accessState} onEnter={() => { void openSettings('License').catch((e) => (error = String(e))); }} showWarning={false} />{/if}
    <SpeakersCard {session} {onUpdate} />
  </header>

  <div role="tablist" aria-label="Session">
    {#each tabs as [id, label] (id)}
      <button role="tab" aria-selected={tab === id} disabled={id === 'audio' && !!noAudio}
        title={id === 'audio' ? noAudio ?? undefined : undefined} onclick={() => showTab(id)}>{label}</button>
    {/each}
  </div>
  {#if noAudio && tab === 'note'}<p class="hint">{noAudio}</p>{/if}

  {#if tab === 'note'}
    <div role="tabpanel" aria-label="Note">
      {#if session.status === 'failed'}
        <p role="alert">{session.error}</p>
      {:else if session.status === 'cancelled'}
        <p>Stopped.</p>
      {/if}
      {#if working}
        <p role="status">{progress?.message ?? (session.status === 'awaitingModel' ? 'Waiting for the summary model download…' : 'Writing note…')}</p>
        {#if session.status === 'summarizing' || session.status === 'awaitingModel'}<button onclick={stop}>{session.status === 'summarizing' ? 'Stop' : 'Cancel'}</button>{/if}
      {/if}
      {#if summaryAvailabilityError}
        <p role="alert">Couldn't check summary availability.</p>
        <button disabled={summaryAvailabilityRetrying} onclick={() => retrySummaryAvailability?.()}>Check again</button>
      {/if}
      {#if unavailable}
        <SummaryUnavailable loadFailed={summaryAvailability === 'loadFailed'} retrying={modelRetrying} onRetry={retryModel} />
        <button onclick={copyTranscript} disabled={!session.transcript}>{transcriptCopied ? 'Copied' : 'Copy Transcript'}</button>
      {/if}
      {#if !unavailable || session.summary}
        <div class="actions">
          <button onclick={copy} disabled={!session.summary || working}>{copied ? 'Copied' : 'Copy Note'}</button>
          {#if prompts}
            <StylePicker prompts={prompts.prompts} selectedId={selectedPrompt} disabled={!canRewrite || unavailable} onSelect={(id) => void rewrite(id)} coachEntry={coach ? newStyle : undefined} />
          {/if}
          {#snippet newStyle()}
            {#if coach}<CoachEntry {availability} onOpen={(opener) => coach.open({}, opener)} showRetry />{/if}
          {/snippet}
        </div>
      {/if}
      {#if undoLine && session.previousSummary}
        <p role="status" class="undo">Note rewritten. <button onclick={() => void undo()} disabled={!writable}>Undo</button></p>
      {/if}
      {#if coach && active && session.summary && !session.isSample && coach.firstNoteTip(session.id)}
        <aside class="tip">
          <strong>Want notes that look like yours?</strong>
          <p>Give the coach an example of how you write so it can make a style for your future notes.</p>
          <CoachEntry {availability} onOpen={(opener) => coach.open({}, opener, coach.dismissTip)} showRetry />
          <button disabled={coach.tipSaving()} onclick={coach.dismissTip}>Dismiss tip</button>
          {#if coach.tipSaveFailed()}<p role="alert">Couldn’t save the dismissal. Try Dismiss tip again.</p>{/if}
        </aside>
      {/if}
      {#if preview && working}<pre aria-label="Note in progress">{preview}</pre>
      {:else if session.summary && !working}
        {#if writable}
          <textarea aria-label="Note" bind:value={draft} onblur={() => void commitEdit()}></textarea>
        {:else}
          <pre aria-label="Note">{session.summary}</pre>
        {/if}
      {/if}
      {#if error}<p role="alert">{error}</p>{/if}
      {#if active && session.status === 'done' && session.summary}
        {#key session.id}<SessionCoach {session} {availability} onUpdate={coachAccepted} />{/key}
      {/if}
    </div>
  {:else if tab === 'transcript'}
    <div role="tabpanel" aria-label="Transcript">
      <button onclick={copyTranscript} disabled={!session.transcript}>{transcriptCopied ? 'Copied' : 'Copy Transcript'}</button>
      {#if session.diarizationState === 'confirmed'}
        <SpeakersCard {session} part="transcript" />
      {:else if session.transcript}
        <pre>{session.transcript}</pre>
      {:else}
        <p>{working ? 'The transcript appears here when it is saved.' : 'No transcript.'}</p>
      {/if}
    </div>
  {:else}
    <div role="tabpanel" aria-label="Audio">
      <SessionAudio {session} {active} />
    </div>
  {/if}
</article>

<style>
  pre { white-space: pre-wrap; }
  textarea { width: 100%; min-height: 16em; font: inherit; box-sizing: border-box; }
  [role='tablist'] { display: flex; gap: 4px; margin-block: 8px; }
  [role='tab'][aria-selected='true'] { font-weight: 600; }
  .actions { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
  .hint { opacity: .7; font-size: .9em; }
  .tip { border: 1px solid #8885; border-radius: 6px; padding: 10px; margin-block: 8px; }
  .tip p { margin-block: 5px; }
</style>
