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
  let notice = $state('');
  let view = $state<'Changes' | 'Before' | 'After'>('Changes');
  const beforeLines = $derived(session.summary.split('\n'));
  const afterLines = $derived(candidate?.note.split('\n') ?? []);
  let unlisten: (() => void) | null = null;
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

  $effect(() => () => { sequence++; unlisten?.(); if (job) void sessionCoachStop(job); });

  async function send() {
    const request = input.trim();
    if (!request || pending || blocked) return;
    const base = session.summary;
    const current = ++sequence;
    pending = true;
    notice = '';
    input = '';
    messages = [...messages, { request }];
    try {
      const started = await sessionCoachStart(session.id, request, (result) => {
        if (current !== sequence || result.sessionId !== session.id || (job && job !== result.jobId)) return;
        pending = false;
        job = null;
        unlisten?.(); unlisten = null;
        if (result.outcome.kind === 'candidate') {
          candidate = { id: result.outcome.candidateId, note: result.outcome.note, base };
          view = 'Changes';
          requests = [...requests, request];
        } else {
          const reply = result.outcome.message;
          messages = messages.map((message, index) => index === messages.length - 1 ? { ...message, reply } : message);
        }
      }, (stopped) => {
        if (current !== sequence || stopped.sessionId !== session.id || (job && job !== stopped.jobId)) return;
        pending = false; job = null; notice = 'Stopped.';
        unlisten?.(); unlisten = null;
      }, requests, candidate?.id ?? null);
      if (current === sequence && pending) { job = started.jobId; unlisten = started.unlisten; }
      else started.unlisten();
    } catch (error) {
      if (current === sequence) { pending = false; job = null; notice = String(error); input = request; }
    }
  }

  async function stop() {
    const id = job;
    sequence++;
    pending = false;
    job = null;
    unlisten?.(); unlisten = null;
    notice = 'Stopped.';
    if (id) try { await sessionCoachStop(id); } catch (error) { notice = String(error); }
  }

  async function accept() {
    if (!candidate) return;
    try {
      const updated = await sessionCoachAccept(session.id, candidate.id);
      candidate = null;
      onUpdate?.(updated);
      notice = 'Revision saved.';
    } catch (error) {
      notice = String(error);
      if (notice.includes('changed') || notice.includes('no longer')) candidate = null;
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
          {#each beforeLines.filter((line) => line.trim() && !afterLines.includes(line)) as line}<del>{line}</del>{/each}
          {#each afterLines as line}
            {#if line.trim() && !beforeLines.includes(line)}<ins>{line}</ins>{:else}<span>{line || ' '}</span>{/if}
          {/each}
        </div>
      {/if}
      <button disabled={pending || blocked} onclick={accept}>Use This</button>
      <button disabled={pending} onclick={() => { candidate = null; notice = 'Kept the old note.'; }}>Keep the Old One</button>
    </section>
  {/if}
  {#if notice}<p role="status">{notice}</p>{/if}
  {#if blocked}<p id="coach-reason">{reason}</p>{/if}
  {#if availability?.retry === 'tryAgain'}<button onclick={() => void retrySummaryModel().catch((error) => (notice = String(error)))}>Try Again</button>{/if}
  <form onsubmit={(event) => { event.preventDefault(); if (pending) void stop(); else void send(); }}>
    <label for="coach-request">Ask Coach to change the note</label>
    <textarea id="coach-request" bind:value={input} disabled={blocked} aria-describedby={blocked ? 'coach-reason' : undefined}></textarea>
    <button disabled={!pending && (blocked || !input.trim())}>{pending ? 'Stop' : 'Send'}</button>
  </form>
</section>

<style>
  pre, .diff { white-space: pre-wrap; overflow-wrap: anywhere; }
  .diff > * { display: block; }
</style>
