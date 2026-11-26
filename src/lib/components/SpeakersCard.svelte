<script lang="ts" module>
  // R14: unsaved speaker choices survive closing and reopening the session (per app run).
  const drafts = new Map<string, Record<string, string>>();
</script>

<script lang="ts">
  import { onDestroy } from 'svelte';
  import { confirmSpeakerLabels, getSession, skipDiarization, updateSpeakerMap } from '../api';
  import type { Session } from '../types/Session';
  import { accessState, allows } from '../license';
  type SpeakerMap = Record<string, string>;

  // part='transcript' renders only the speaker-labelled transcript (Transcript tab).
  let { session: incoming, onUpdate, part = 'controls' }: { session: Session; onUpdate?: (session: Session) => void; part?: 'controls' | 'transcript' } = $props();
  let current = $state<Session | null>(null);
  let overrides = $state<SpeakerMap | null>(null);
  let editing = $state(false);
  let addingLater = $state(false);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let disposed = false;
  onDestroy(() => { disposed = true; });
  $effect(() => {
    current = incoming;
    overrides = drafts.get(incoming.id) ?? null;
    editing = incoming.diarizationState === 'pending' || (overrides !== null && incoming.diarizationState === 'confirmed');
  });
  const writable = $derived(allows($accessState));
  let session = $derived(current ?? incoming);
  let names = $derived(overrides ?? session.speakerMap ?? {});
  let speakers = $derived(Object.entries(names));

  function quote(raw: string | null, id: string) {
    const line = raw?.split('\n').find((row) => row.startsWith(`${id}:`));
    return line?.slice(id.length + 1).trim().slice(0, 100) ?? '';
  }

  function colorIndex(line: string) {
    const name = line.slice(0, line.indexOf(':'));
    return Math.max(0, Object.values(session.speakerMap ?? {}).indexOf(name)) % 5;
  }

  function setName(id: string, name: string) {
    overrides = { ...names, [id]: name };
    drafts.set(session.id, overrides);
  }

  async function refresh() {
    const id = session.id;
    const updated = await getSession(id);
    if (updated && !disposed && incoming.id === id) {
      current = updated;
      overrides = null;
      drafts.delete(id);
      addingLater = false;
      onUpdate?.(updated);
    }
  }

  async function confirm() {
    busy = true;
    try {
      await confirmSpeakerLabels(session.id, names);
      editing = false;
      await refresh();
    } catch (e) { error = String(e); }
    finally { busy = false; }
  }

  async function skip() {
    busy = true;
    try { await skipDiarization(session.id); await refresh(); }
    catch (e) { error = String(e); }
    finally { busy = false; }
  }

  /** Names apply only on Done (R14). */
  async function done() {
    if (!overrides) { editing = false; return; }
    busy = true;
    try { await updateSpeakerMap(session.id, names); editing = false; await refresh(); }
    catch (e) { error = String(e); }
    finally { busy = false; }
  }
</script>

{#if part === 'controls'}
{#if session.rawTranscript && (session.diarizationState === 'pending' || session.diarizationState === 'confirmed' || addingLater)}
  {#if session.diarizationState === 'confirmed' && !editing}
    <button onclick={() => (editing = true)} disabled={!writable}>Edit Speakers</button>
  {:else}
    {@const locked = session.diarizationState === 'confirmed' && !writable}
    <section aria-labelledby="speakers-title">
      <h4 id="speakers-title">Who's speaking in this session?</h4>
      {#each speakers as [id, name], i (id)}
        <div class="speaker color-{i % 5}">
          <label>{id}<input value={name} disabled={locked} oninput={(event) => setName(id, event.currentTarget.value)} /></label>
          <p>{quote(session.rawTranscript, id)}</p>
          <button disabled={locked} onclick={() => setName(id, 'You')}>Me</button>
          <button disabled={locked} onclick={() => setName(id, 'Client')}>Client</button>
          {#each speakers.filter(([other]) => other !== id) as [other, otherName] (other)}
            <button disabled={locked} onclick={() => setName(id, otherName)}>{otherName}</button>
          {/each}
        </div>
      {/each}
      {#if session.diarizationState === 'pending' || addingLater}
        {@const gated = addingLater && !writable}
        <button onclick={confirm} disabled={busy || gated}>Confirm speakers</button>
        <button onclick={skip} disabled={busy || gated}>Skip</button>
      {:else}
        <button onclick={done} disabled={busy || (locked && overrides !== null)}>Done</button>
      {/if}
    </section>
  {/if}
{:else if session.rawTranscript && session.diarizationState === 'skipped'}
  <button disabled={!writable} onclick={() => { addingLater = true; editing = true; }}>Add speakers</button>
{/if}
{:else if session.diarizationState === 'confirmed' && session.transcript}
  <div class="transcript" aria-label="Speaker-labelled transcript">
    {#each session.transcript.split('\n') as line, i (i)}
      <p class="color-{colorIndex(line)}">{line}</p>
    {/each}
  </div>
{/if}
{#if error}<p role="alert">{error}</p>{/if}

<style>
  .speaker { border-left: 4px solid; padding: .5rem; margin: .5rem 0; }
  .speaker label { display: flex; gap: .5rem; }
  .color-0 { border-color: #5973d9; color: #263d9d; }
  .color-1 { border-color: #bc5e6f; color: #8b2f40; }
  .color-2 { border-color: #4b9971; color: #286b49; }
  .color-3 { border-color: #c28c31; color: #805916; }
  .color-4 { border-color: #8964bc; color: #60418c; }
  .transcript p { border-left: 3px solid; padding-left: .5rem; }
</style>
