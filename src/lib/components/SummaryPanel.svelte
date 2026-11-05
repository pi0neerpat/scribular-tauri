<script lang="ts">
  import type { Session } from '../types/Session';
  import SessionAudio from './SessionAudio.svelte';
  import SpeakersCard from './SpeakersCard.svelte';

  // `active` is false while the record view is hidden behind Settings.
  let {
    session,
    active = true,
    byline = '',
    onUpdate,
  }: { session: Session; active?: boolean; byline?: string; onUpdate?: (session: Session) => void } = $props();
</script>

<article>
  <h3>{session.title}</h3>
  {#if session.status === 'awaitingModel'}
    <p>Waiting for the summary model download…</p>
  {:else if session.status === 'failed'}
    <p role="alert">{session.error}</p>
  {:else if session.status === 'cancelled'}
    <p>Stopped.</p>
  {/if}
  {#if session.summary}
    <h4>Summary{#if byline} · {byline}{/if}</h4>
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
