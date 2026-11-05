<script lang="ts">
  import { confirmSpeakerLabels, getSession, skipDiarization, updateSpeakerMap } from '../api';
  import type { Session } from '../types/Session';
  type SpeakerMap = Record<string, string>;

  let { session: incoming, onUpdate }: { session: Session; onUpdate?: (session: Session) => void } = $props();
  let current = $state<Session | null>(null);
  let overrides = $state<SpeakerMap | null>(null);
  let editing = $state(false);
  let addingLater = $state(false);
  let busy = $state(false);
  let error = $state<string | null>(null);
  $effect(() => {
    current = incoming;
    editing = incoming.diarizationState === 'pending';
  });
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
  }

  async function refresh() {
    const updated = await getSession(session.id);
    if (updated) {
      current = updated;
      overrides = null;
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

  async function rename() {
    try { await updateSpeakerMap(session.id, names); await refresh(); }
    catch (e) { error = String(e); }
  }
</script>

{#if session.rawTranscript && (session.diarizationState === 'pending' || session.diarizationState === 'confirmed' || addingLater)}
  {#if session.diarizationState === 'confirmed' && !editing}
    <button onclick={() => (editing = true)}>Edit speakers</button>
  {:else}
    <section aria-labelledby="speakers-title">
      <h4 id="speakers-title">Who's speaking in this session?</h4>
      {#each speakers as [id, name], i (id)}
        <div class="speaker color-{i % 5}">
          <label>{id}<input value={name} oninput={(event) => setName(id, event.currentTarget.value)} onblur={session.diarizationState === 'confirmed' ? rename : undefined} /></label>
          <p>{quote(session.rawTranscript, id)}</p>
          <button onclick={() => { setName(id, 'You'); if (session.diarizationState === 'confirmed') rename(); }}>Me</button>
          <button onclick={() => { setName(id, 'Client'); if (session.diarizationState === 'confirmed') rename(); }}>Client</button>
          {#each speakers.filter(([other]) => other !== id) as [other, otherName] (other)}
            <button onclick={() => { setName(id, otherName); if (session.diarizationState === 'confirmed') rename(); }}>{otherName}</button>
          {/each}
        </div>
      {/each}
      {#if session.diarizationState === 'pending' || addingLater}
        <button onclick={confirm} disabled={busy}>Confirm speakers</button>
        <button onclick={skip} disabled={busy}>Skip</button>
      {:else}
        <button onclick={() => (editing = false)}>Done</button>
      {/if}
    </section>
  {/if}
{:else if session.rawTranscript && session.diarizationState === 'skipped'}
  <button onclick={() => { addingLater = true; editing = true; }}>Add speakers</button>
{/if}

{#if session.diarizationState === 'confirmed' && session.transcript}
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
