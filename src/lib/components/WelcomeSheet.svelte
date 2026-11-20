<script lang="ts">
  import { onMount } from 'svelte';
  import { completeOnboarding, getOnboardingState, getTelemetrySetting, setTelemetrySetting } from '../api';

  let { open = $bindable(false), suspended = false }: { open?: boolean; suspended?: boolean } = $props();
  let dialog: HTMLDialogElement;
  let firstDisplayPending = $state(false);
  let busy = $state(false);
  let consent = $state(false);
  let error = $state('');
  let retry = $state<(() => void) | null>(null);
  let disabled = $derived(busy || firstDisplayPending || !!error);

  onMount(() => { void load(false); });

  $effect(() => {
    if (!open || suspended) return;
    dialog.showModal();
    return () => dialog.close();
  });

  $effect(() => {
    if (!firstDisplayPending || suspended || busy) return;
    firstDisplayPending = false;
    void saveFirstDisplay();
  });

  export function show() {
    if (!open && !busy) void load(true);
  }

  async function run(action: () => Promise<void>, failure: string, openOnError = true) {
    busy = true;
    try {
      await action();
      error = '';
      retry = null;
    } catch {
      if (openOnError) open = true;
      error = failure;
      retry = () => { void run(action, failure, openOnError); };
    } finally {
      busy = false;
    }
  }

  function load(reopen: boolean) {
    return run(async () => {
      const { completed } = await getOnboardingState();
      if (!completed) {
        open = true;
        firstDisplayPending = true;
      } else if (reopen || open) {
        open = true;
        consent = (await getTelemetrySetting()) === 'on';
      }
    }, 'Could not read your welcome settings.', reopen);
  }

  function saveFirstDisplay() {
    return run(async () => {
      await setTelemetrySetting(false);
      // Leave welcome unseen if saving default-off consent failed.
      await completeOnboarding();
      consent = false;
    }, 'Could not save your welcome settings.');
  }

  function changeConsent(event: Event & { currentTarget: HTMLInputElement }) {
    const next = event.currentTarget.checked;
    event.currentTarget.checked = consent;
    void run(async () => {
      await setTelemetrySetting(next);
      consent = next;
    }, 'Could not save your usage-data choice.');
  }
</script>

{#if error && !open}
  <p role="alert">{error}</p>
  <button onclick={() => retry?.()} disabled={busy}>Retry welcome settings</button>
{/if}
<dialog bind:this={dialog} aria-labelledby="welcome-title" oncancel={(e) => e.preventDefault()}>
  <h2 id="welcome-title">Welcome to Scribular</h2>
  <ul>
    <li><strong>Record a conversation</strong><br />Use your microphone, or record your microphone and a call app like Zoom together.</li>
    <li><strong>Notes in your style</strong><br />Get a transcript and a summary in your chosen note style after you stop.</li>
    <li><strong>Stays on this Mac</strong><br />Scribular transcribes recordings and writes notes on this Mac. Your recordings, transcripts and notes stay here.</li>
  </ul>
  <label>
    <input type="checkbox" checked={consent} {disabled} onchange={changeConsent} aria-describedby="welcome-privacy" />
    Share anonymous usage and crash data
  </label>
  <p id="welcome-privacy">Usage and crash reports contain no audio, transcripts or notes.</p>
  {#if error}
    <p role="alert">{error}</p>
    <button onclick={() => retry?.()} disabled={busy}>Retry</button>
  {/if}
  <button class="continue" {disabled} onclick={() => (open = false)}>Continue</button>
</dialog>

<style>
  dialog {
    box-sizing: border-box;
    width: min(28rem, calc(100% - 2rem));
    max-height: calc(100% - 2rem);
    padding: 1.5rem;
    border: 1px solid GrayText;
    border-radius: 1rem;
    background: Canvas;
    color: CanvasText;
    box-shadow: 0 1rem 3rem #0004;
  }
  dialog::backdrop { background: #0007; }
  h2 { margin-top: 0; }
  ul { list-style: none; padding: 0; margin: 1.25rem 0; display: grid; gap: .85rem; }
  label { display: flex; gap: .5rem; align-items: baseline; }
  #welcome-privacy { color: GrayText; margin: .25rem 0 1rem 1.75rem; font-size: .875rem; }
  .continue { display: block; width: 100%; margin-top: .75rem; padding: .45rem; }
</style>
