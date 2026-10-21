<script lang="ts">
  // The transcript so far, from `session://segments` while the session
  // records and drains. Segments from before a window reload are not shown.
  import { onSessionSegments } from '../api';
  import { liveTranscript, type SourceSegments } from '../transcript';

  let { sessionId, labeled }: { sessionId: string; labeled: boolean } = $props();

  let segments = $state<SourceSegments>({ mic: [], system: [] });
  let text = $derived(liveTranscript(segments, labeled));

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
  <pre>{text}</pre>
{/if}

<style>
  pre {
    white-space: pre-wrap;
  }
</style>
