<script lang="ts">
  // The transcript so far, from `session://segments` while the session
  // records and drains. Segments from before a window reload are not shown.
  import { onSessionSegments } from '../api';
  import { liveTranscript, wordCount, type SourceSegments } from '../transcript';

  let { sessionId, labeled, waiting = false }: { sessionId: string; labeled: boolean; waiting?: boolean } = $props();

  let segments = $state<SourceSegments>({ mic: [], system: [] });
  let text = $derived(liveTranscript(segments, labeled));
  let words = $derived(wordCount(text, labeled));

  $effect(() => {
    const id = sessionId;
    segments = { mic: [], system: [] };
    const unlisten = onSessionSegments((s) => {
      if (s.sessionId === id) segments[s.source].push(...s.segments);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  });
</script>

{#if text}
  <p>{words} {words === 1 ? 'word' : 'words'}</p>
  <pre>{text}</pre>
{:else if waiting}
  <p>Live transcript will start when the current summary finishes.</p>
{/if}

<style>
  pre {
    white-space: pre-wrap;
  }
</style>
