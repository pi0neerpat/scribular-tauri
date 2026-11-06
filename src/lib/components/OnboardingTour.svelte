<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { openExternalLink } from '../api';

  type View = 'record' | 'settings';
  type Step = { id: string; title: string; copy: string; anchor?: string };
  let { onNavigate, onComplete, sessionsEmpty = false }: {
    onNavigate: (view: View) => void;
    onComplete: () => void;
    sessionsEmpty?: boolean;
  } = $props();

  let steps = $state<Step[]>([
    { id: 'welcome', title: 'Welcome', copy: "Let's take a quick tour. Your permissions and speech model are getting ready below." },
    { id: 'audio', title: 'Choose an audio source in Settings', copy: 'Use your microphone, application audio, or both.', anchor: 'input-mode' },
    { id: 'sessions', title: 'View Sessions', copy: 'After you finish a recording, the transcript and summary appear here.', anchor: 'sessions' },
    { id: 'notes', title: 'Customize Your Notes', copy: 'Edit your note template in Settings.', anchor: 'settings' },
  ]);
  let index = $state(0);
  let card = $state<HTMLElement | undefined>();
  let position = $state('right: 1rem; top: 1rem;');
  let supportError = $state<string | null>(null);
  let current = $derived(steps[index]);

  function place() {
    if (!current?.anchor) {
      position = 'right: 1rem; top: 1rem;';
      return;
    }
    const anchor = document.querySelector<HTMLElement>(`[data-tour="${current.anchor}"]`);
    if (!anchor) return;
    const target = current.anchor === 'input-mode' ? anchor.querySelector<HTMLElement>('fieldset') ?? anchor : anchor;
    const rect = target.getBoundingClientRect();
    const width = Math.min(320, window.innerWidth - 32);
    const left = Math.min(window.innerWidth - width - 16, Math.max(16, rect.right + 16));
    const top = Math.max(16, Math.min(window.innerHeight - (card?.offsetHeight ?? 0) - 16, rect.top));
    position = `left: ${left}px; top: ${top}px;`;
  }

  async function next() {
    if (current.id === 'welcome') onNavigate('settings');
    index = Math.min(index + 1, steps.length - 1);
    await tick();
    place();
    card?.querySelector<HTMLElement>('button:not(.close)')?.focus();
  }

  function keydown(event: KeyboardEvent) {
    if (event.key === 'Escape') onComplete();
  }

  async function contactSupport(event: MouseEvent) {
    event.preventDefault();
    try {
      await openExternalLink('mailto:support@scribular.com');
      supportError = null;
    } catch (e) {
      supportError = String(e);
    }
  }

  onMount(() => {
    steps = steps.filter((step) => step.id !== 'sessions' || document.querySelector('[data-tour="sessions"]'));
    if (sessionsEmpty) {
      const sessionStep = steps.find((step) => step.id === 'sessions');
      if (sessionStep) sessionStep.copy = 'Your transcript and summary will show up here after you record. Everything stays on your device.';
    }
    void tick().then(() => {
      card?.focus();
      place();
    });
    window.addEventListener('resize', place);
    window.addEventListener('scroll', place, true);
    const observer = new MutationObserver(place);
    observer.observe(document.body, { childList: true, subtree: true });
    return () => {
      window.removeEventListener('resize', place);
      window.removeEventListener('scroll', place, true);
      observer.disconnect();
    };
  });
</script>

<svelte:window onkeydown={keydown} />

{#if current}
  <aside bind:this={card} class="tour-card" style={`${position} pointer-events: none;`} tabindex="-1" aria-label="Quick tour">
    <button class="close" aria-label="Close tour" onclick={onComplete}>×</button>
    <p class="step">{index + 1} of {steps.length}</p>
    <h2>{current.title}</h2>
    <p>{current.copy}</p>
    {#if current.id === 'notes'}
      <p>Need help? Reach out at <a href="mailto:support@scribular.com" onclick={contactSupport}>support@scribular.com</a>.</p>
      {#if supportError}<p role="alert">{supportError}</p>{/if}
      <button onclick={onComplete}>Done</button>
    {:else}
      <button onclick={next}>Next</button>
    {/if}
  </aside>
{/if}

<style>
  .tour-card { position: fixed; z-index: 20; box-sizing: border-box; width: min(320px, calc(100vw - 2rem)); padding: 1.25rem; border: 1px solid #c9ced8; border-radius: 0.75rem; background: white; color: #172033; box-shadow: 0 8px 28px #17203333; pointer-events: none; }
  .tour-card button, .tour-card a { pointer-events: auto; }
  .tour-card h2 { margin: 0.4rem 2rem 0.5rem 0; font-size: 1.1rem; }
  .tour-card p { margin: 0.5rem 0 1rem; line-height: 1.45; }
  .step { color: #58657c; font-size: 0.8rem; }
  .close { position: absolute; top: 0.5rem; right: 0.5rem; border: 0; background: transparent; font-size: 1.4rem; }
  .tour-card button:not(.close) { min-height: 2.5rem; padding: 0.4rem 1rem; }
</style>
