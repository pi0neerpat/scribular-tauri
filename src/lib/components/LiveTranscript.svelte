<script lang="ts">
  // The transcript so far, from `session://segments` while the session
  // records and drains. Segments from before a window reload are not shown.
  import { onSessionSegments } from '../api';
  import { liveTranscript, wordCount, type SourceSegments } from '../transcript';

  let { sessionId, labeled, waiting = false, activeElapsedMs = 0 }: { sessionId: string; labeled: boolean; waiting?: boolean; activeElapsedMs?: number } = $props();

  let segments = $state<SourceSegments>({ mic: [], system: [] });
  let text = $derived(liveTranscript(segments, labeled));
  let manuallyVisible = $state<boolean | null>(null);
  // Active time when text first showed; transcription arrives in batches, so keep it up 20s before auto-hiding.
  let shownAtMs = $state<number | null>(null);
  let collapsed = $derived(manuallyVisible === null
    ? shownAtMs !== null && activeElapsedMs - shownAtMs >= 20_000
      && (activeElapsedMs >= 60_000 || (wordCount(liveTranscript({ mic: segments.mic, system: [] }, false), false) >= 20
        && (!labeled || wordCount(liveTranscript({ mic: [], system: segments.system }, false), false) >= 20)))
    : !manuallyVisible);

  $effect(() => {
    if (text && shownAtMs === null) shownAtMs = activeElapsedMs;
  });

  $effect(() => {
    const id = sessionId;
    segments = { mic: [], system: [] };
    manuallyVisible = null;
    shownAtMs = null;
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
