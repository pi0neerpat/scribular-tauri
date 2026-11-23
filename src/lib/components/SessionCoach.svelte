<script lang="ts">
  import { retrySummaryModel, sessionCoachAccept, sessionCoachStart, sessionCoachStop } from '../api';
  import type { CoachAvailability } from '../types/CoachAvailability';
  import type { Session } from '../types/Session';

  let { session, availability, onUpdate }: {
    session: Session;
    availability: CoachAvailability | null;
    onUpdate?: (session: Session) => void;
  } = $props();
  let input = $state('');
  let requests = $state<string[]>([]);
  let messages = $state<{ request: string; reply?: string }[]>([]);
  let candidate = $state<{ id: string; note: string; base: string } | null>(null);
  let job = $state<string | null>(null);
  let pending = $state(false);
  let stopping = $state(false);
  let accepting = $state(false);
  let notice = $state('');
  let view = $state<'Changes' | 'Before' | 'After'>('Changes');
  const beforeLines = $derived(session.summary.split('\n'));
  const afterLines = $derived(candidate?.note.split('\n') ?? []);
  const changes = $derived.by(() => {
    const before = beforeLines, after = afterLines;
    const lengths = Array.from({ length: before.length + 1 }, () => Array<number>(after.length + 1).fill(0));
    for (let i = before.length - 1; i >= 0; i--)
      for (let j = after.length - 1; j >= 0; j--)
        lengths[i][j] = before[i] === after[j] ? lengths[i + 1][j + 1] + 1 : Math.max(lengths[i + 1][j], lengths[i][j + 1]);
    const rows: { kind: 'same' | 'removed' | 'added'; line: string }[] = [];
    let i = 0, j = 0;
    while (i < before.length || j < after.length) {
      if (i < before.length && j < after.length && before[i] === after[j]) rows.push({ kind: 'same', line: before[i++] }), j++;
      else if (i < before.length && (j === after.length || lengths[i + 1][j] >= lengths[i][j + 1])) rows.push({ kind: 'removed', line: before[i++] });
      else rows.push({ kind: 'added', line: after[j++] });
    }
    return rows;
  });
  let unlisten: (() => void) | null = null;
  let starting: ReturnType<typeof sessionCoachStart> | null = null;
  let sequence = 0;
  const blocked = $derived(!availability || availability.reason !== null);
  const reason = $derived(availability?.sessionMessage ?? 'Checking whether the coach is available…');

  $effect(() => {
    if (blocked && availability?.reason !== 'busy' && pending && job) void stop();
  });

  $effect(() => {
    if (candidate && candidate.base !== session.summary) {
      candidate = null;
      notice = 'The note changed. Ask for a new revision to review.';
    }
  });

  $effect(() => () => {
    sequence++;
    unlisten?.();
    if (stopping) return;
    if (job) void sessionCoachStop(job).catch(() => {});
    else void starting?.then(({ jobId }) => sessionCoachStop(jobId)).catch(() => {});
  });

  async function send() {
    const request = input.trim();
    if (!request || pending || stopping || accepting || blocked) return;
    const base = session.summary;
    const priorRequests = candidate ? requests : [];
    const current = ++sequence;
    pending = true;
    notice = '';
    input = '';
    messages = [...messages, { request }];
    try {
      starting = sessionCoachStart(session.id, request, (result) => {
        if (current !== sequence || result.sessionId !== session.id || (job && job !== result.jobId)) return;
        pending = false;
        job = null;
        unlisten?.(); unlisten = null;
        if (result.outcome.kind === 'candidate') {
          candidate = { id: result.outcome.candidateId, note: result.outcome.note, base };
          view = 'Changes';
          requests = [...priorRequests, request].slice(-10);
        } else {
          const reply = result.outcome.message;
          messages = messages.map((message, index) => index === messages.length - 1 ? { ...message, reply } : message);
        }
      }, (stopped) => {
        if (current !== sequence || stopped.sessionId !== session.id || (job && job !== stopped.jobId)) return;
        pending = false; job = null; notice = 'Stopped.';
        unlisten?.(); unlisten = null;
      }, priorRequests, candidate?.id ?? null);
      const started = await starting;
      if (current === sequence && pending) { job = started.jobId; unlisten = started.unlisten; }
      else started.unlisten();
    } catch (error) {
      if (current === sequence) { pending = false; job = null; notice = String(error); input = request; }
    } finally {
      starting = null;
    }
  }

  async function stop() {
    if (stopping) return;
    const id = job;
    const start = starting;
    sequence++;
    stopping = true;
    pending = false;
    job = null;
    unlisten?.(); unlisten = null;
    try {
      const jobId = id ?? (await start)?.jobId;
      if (jobId) await sessionCoachStop(jobId);
      notice = 'Stopped.';
    } catch (error) {
      notice = String(error);
    } finally {
      stopping = false;
    }
  }

  async function accept() {
    if (!candidate || pending || stopping || blocked || accepting) return;
    accepting = true;
    try {
      const updated = await sessionCoachAccept(session.id, candidate.id);
      candidate = null;
      onUpdate?.(updated);
      notice = 'Revision saved.';
    } catch (error) {
      notice = String(error);
      if (notice.includes('changed') || notice.includes('no longer')) candidate = null;
    } finally {
      accepting = false;
    }
  }
</script>

<section aria-label="Session Coach">
  <h4>Coach</h4>
  {#each messages as message}
    <p><strong>You:</strong> {message.request}</p>
    {#if message.reply}<p role="status">{message.reply}</p>{/if}
  {/each}
  {#if candidate}
    <section aria-label="Candidate note">
      <nav aria-label="Compare notes">
        {#each ['Changes', 'Before', 'After'] as tab}
          <button aria-pressed={view === tab} onclick={() => (view = tab as typeof view)}>{tab}</button>
        {/each}
      </nav>
      {#if view === 'Before'}<pre>{session.summary}</pre>
      {:else if view === 'After'}<pre>{candidate.note}</pre>
      {:else}
        <div class="diff" aria-label="Note changes">
          {#each changes as change}
            {#if change.kind === 'removed'}<del>{change.line || ' '}</del>
            {:else if change.kind === 'added'}<ins>{change.line || ' '}</ins>
            {:else}<span>{change.line || ' '}</span>{/if}
          {/each}
        </div>
      {/if}
      <button disabled={pending || stopping || blocked || accepting} onclick={accept}>Use This</button>
      <button disabled={pending || stopping || accepting} onclick={() => { candidate = null; notice = 'Kept the old note.'; }}>Keep the Old One</button>
    </section>
  {/if}
  {#if notice}<p role="status">{notice}</p>{/if}
  {#if blocked}<span id="coach-reason" hidden>{reason}</span>{/if}
  {#if availability?.retry === 'tryAgain'}<button onclick={() => void retrySummaryModel().catch((error) => (notice = String(error)))}>Try Again</button>{/if}
  <form onsubmit={(event) => { event.preventDefault(); if (pending) void stop(); else void send(); }}>
    <label for="coach-request">Ask Coach to change the note</label>
    <textarea id="coach-request" value={blocked ? '' : input} oninput={(event) => (input = event.currentTarget.value)}
      disabled={blocked} placeholder={blocked ? reason : undefined} aria-describedby={blocked ? 'coach-reason' : undefined}></textarea>
    <button disabled={stopping || accepting || (!pending && (blocked || !input.trim()))}>{pending || stopping ? 'Stop' : 'Send'}</button>
  </form>
</section>

<style>
  pre, .diff { white-space: pre-wrap; overflow-wrap: anywhere; }
  .diff > * { display: block; }
</style>
