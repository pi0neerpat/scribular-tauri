<script lang="ts">
  // The transcript so far, from `session://segments` while the session
  // records and drains. Segments from before a window reload are not shown.
  import { onSessionSegments } from '../api';
  import { liveTranscript, wordCount, type SourceSegments } from '../transcript';

  let { sessionId, labeled, waiting = false, activeElapsedMs = 0 }: { sessionId: string; labeled: boolean; waiting?: boolean; activeElapsedMs?: number } = $props();

  let segments = $state<SourceSegments>({ mic: [], system: [] });
  let text = $derived(liveTranscript(segments, labeled));
  let manuallyVisible = $state<boolean | null>(null);
  let collapsed = $derived(manuallyVisible === null
    ? activeElapsedMs >= 60_000 || (wordCount(segments.mic.map((s) => s.text).join(' '), false) >= 5
      && (!labeled || wordCount(segments.system.map((s) => s.text).join(' '), false) >= 5))
    : !manuallyVisible);

  $effect(() => {
    const id = sessionId;
    segments = { mic: [], system: [] };
    manuallyVisible = null;
    const unlisten = onSessionSegments((s) => {
      if (s.sessionId === id) segments[s.source].push(...s.segments);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  });
</script>

{#if text}
  <button onclick={() => (manuallyVisible = collapsed)}> {collapsed ? 'Show transcript' : 'Hide transcript'} </button>
  {#if !collapsed}<pre>{text}</pre>{/if}
{:else if waiting}
  <p>Live transcript will start when the current summary finishes.</p>
{/if}

<style>
  pre {
    white-space: pre-wrap;
  }
</style>
