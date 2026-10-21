<script lang="ts">
  // KTD16 stage timings, copied as a Markdown table for the scorecard.
  import type { Timings } from '../types/Timings';

  let { timings }: { timings: Timings } = $props();

  let rows = $derived<[string, number | null][]>([
    ['Tail transcription', timings.tailTranscriptionMs],
    ['Model load', timings.modelLoadMs],
    ...timings.chunkMs.map((ms, i): [string, number] => [`Chunk ${i + 1}`, ms]),
    ['Combine', timings.combineMs],
    ['Save', timings.saveMs],
  ]);
  // The chunks decode in one batch and each entry is its finish time from the
  // batch start, so the largest one is the chunk stage; summing would overstate it.
  let total = $derived(
    (timings.tailTranscriptionMs ?? 0) +
      (timings.modelLoadMs ?? 0) +
      Math.max(0, ...timings.chunkMs) +
      (timings.combineMs ?? 0) +
      (timings.saveMs ?? 0),
  );
  let copied = $state(false);

  function markdown(): string {
    const lines = rows.map(([stage, ms]) => `| ${stage} | ${ms ?? '–'} |`);
    return ['| Stage | ms |', '|---|---|', ...lines, `| Total | ${total} |`].join('\n');
  }

  async function copy() {
    await navigator.clipboard.writeText(markdown());
    copied = true;
    setTimeout(() => (copied = false), 1500);
  }
</script>

<section>
  <h3>Timings</h3>
  <table>
    <tbody>
      {#each rows as [stage, ms] (stage)}
        <tr><td>{stage}</td><td>{ms ?? '–'} ms</td></tr>
      {/each}
      <tr><th>Total</th><th>{total} ms</th></tr>
    </tbody>
  </table>
  <button onclick={copy}>{copied ? 'Copied' : 'Copy'}</button>
</section>
