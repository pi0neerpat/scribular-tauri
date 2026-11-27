<script lang="ts">
  // Theme R7/R8/R10: the window picture. Only bundled files and the app-owned
  // scheme are shown; Stars is the live canvas.
  import BackgroundAnimation from './BackgroundAnimation.svelte';
  import { backgroundSource } from '../appearance';
  import { reportPictureUnavailable } from '../appearance-store';
  import type { Background } from '../types/Background';
  let { background, motion, recording, blur, brightness }: { background: Background; motion: boolean; recording: boolean; blur: number; brightness: number } = $props();
  let source = $derived(backgroundSource(background));
  let failed = $state<string | null>(null);
  // R12: a missing or corrupt picture leaves the solid window background, and
  // an owned one tells the user to choose it again.
  let shown = $derived(source && failed !== source ? source : null);
  function failedToLoad(src: string | null) {
    failed = src;
    if (src && typeof background === 'object' && src === source) reportPictureUnavailable();
  }
</script>

{#if background === 'stars'}
  <BackgroundAnimation {motion} {recording} />
{:else if shown}
  {#key shown}
    <img class="theme-picture" src={shown} alt="" aria-hidden="true" onerror={(e) => failedToLoad(e.currentTarget.getAttribute('src'))}
      style:filter={`blur(${Math.min(100, blur) * 0.3}px) brightness(${Math.min(100, brightness)}%)`} />
  {/key}
{/if}

<style>
  .theme-picture { position: fixed; inset: -40px; width: calc(100% + 80px); height: calc(100vh + 80px); object-fit: cover; pointer-events: none; z-index: -1; }
</style>
