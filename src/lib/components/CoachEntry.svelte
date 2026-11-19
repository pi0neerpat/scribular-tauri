<script lang="ts">
  import { openSettings, retrySummaryModel } from '../api';
  import type { CoachAvailability } from '../types/CoachAvailability';

  let { availability, label = 'New Style from Example Note…', onOpen, showRetry = false }:
    { availability: CoachAvailability | null; label?: string; onOpen: (opener: HTMLElement) => void; showRetry?: boolean } = $props();
  const descriptionId = $props.id();
  let blocked = $derived(!availability || availability.reason !== null);
  let message = $derived(availability?.message ?? (blocked ? 'Checking whether the coach is available…' : ''));
  let retrying = $state(false);
  let retryError = $state(false);

  $effect(() => {
    availability;
    retryError = false;
  });

  async function retry() {
    const requested = availability;
    retrying = true;
    retryError = false;
    try {
      if (requested?.retry === 'license') await openSettings('License');
      else if (requested?.retry === 'tryAgain') await retrySummaryModel();
    } catch {
      if (availability === requested) retryError = true;
    } finally {
      retrying = false;
    }
  }
</script>

<span class="entry" title={message || undefined}>
  <button
    aria-disabled={blocked}
    aria-describedby={blocked ? descriptionId : undefined}
    onclick={(event) => { if (!blocked) onOpen(event.currentTarget); }}
  >{label}</button>
  {#if blocked}<span id={descriptionId}>{message}</span>{/if}
</span>
{#if showRetry && availability?.retry}
  <button disabled={retrying} onclick={retry}>{availability.retry === 'license' ? 'Enter License…' : 'Try Again'}</button>
{/if}
{#if retryError && showRetry && availability?.retry}<span role="alert">Couldn’t try again. Please try in a moment.</span>{/if}

<style>
  .entry { display: inline-block; }
  button[aria-disabled='true'] { opacity: 0.52; cursor: default; }
</style>
