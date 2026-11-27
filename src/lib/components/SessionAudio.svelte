<script lang="ts">
  // The session's kept audio. Owns the blob URL: revoked when the session
  // changes, the player unmounts or hides, or the audio expires, so the WAV
  // lives only in webview memory and never outlives its retention deadline.
  import { getSessionSegments } from '../api';
  import type { Session } from '../types/Session';
  import type { TranscriptSegment } from '../types/TranscriptSegment';
  import { audioPending, currentLine, formatTime, openAudio, retentionLine, SPEEDS, type AudioView } from './session-audio';

  let { session, active = true }: { session: Session; active?: boolean } = $props();

  let now = $state(Date.now());
  // Wakes at the deadline; capped because setTimeout overflows past ~24.8 days.
  $effect(() => {
    const at = session.audioExpiresAt;
    if (at === null || at <= Date.now()) return;
    const t = setTimeout(() => (now = Date.now()), Math.min(at - Date.now(), 2 ** 31 - 1));
    return () => clearTimeout(t);
  });
  let expired = $derived(session.audioExpiresAt !== null && now >= session.audioExpiresAt);

  // Primitive, so a refreshed `session` object with the same id doesn't refetch.
  let id = $derived(session.hasAudio && active && !expired ? session.id : null);
  let view = $state<AudioView>({ kind: 'hidden' });
  let lines = $state<TranscriptSegment[]>([]);
  let linesFailed = $state(false);
  let audio = $state<HTMLAudioElement | undefined>();
  let playing = $state(false);
  let position = $state(0);
  let duration = $state(0);
  let speed = $state(1);
  let highlighted = $derived(currentLine(lines, position * 1000));

  $effect(() => {
    if (!id) return;
    let live = true;
    let url: string | null = null;
    view = { kind: 'hidden' };
    openAudio(id).then((v) => {
      if (v.kind === 'player') url = v.url;
      if (live) view = v;
      else if (url) URL.revokeObjectURL(url);
    });
    return () => {
      live = false;
      if (url) URL.revokeObjectURL(url);
      view = { kind: 'hidden' };
      lines = [];
      linesFailed = false;
      playing = false;
      position = 0;
    };
  });

  // Refetched on each status step so lines saved during finalization appear.
  let status = $derived(session.status);
  $effect(() => {
    if (!id) return;
    void status;
    let live = true;
    getSessionSegments(id).then((l) => { if (live) { lines = l; linesFailed = false; } }).catch(() => { if (live) linesFailed = true; });
    return () => { live = false; };
  });

  $effect(() => { if (audio) audio.playbackRate = speed; });

  function seek(seconds: number) {
    if (!audio) return;
    audio.currentTime = Math.max(0, Math.min(seconds, duration || seconds));
    position = audio.currentTime;
  }

  function toggle() {
    if (!audio) return;
    if (audio.paused) void audio.play().catch(() => {}); else audio.pause();
  }
</script>

{#if view.kind === 'player'}
  <section aria-label="Audio player">
    <audio bind:this={audio} src={view.url}
      onplay={() => (playing = true)} onpause={() => (playing = false)} onended={() => (playing = false)}
      ontimeupdate={(e) => (position = e.currentTarget.currentTime)}
      onloadedmetadata={(e) => (duration = Number.isFinite(e.currentTarget.duration) ? e.currentTarget.duration : 0)}></audio>
    <button onclick={toggle}>{playing ? 'Pause' : 'Play'}</button>
    <button aria-label="Back 15 seconds" onclick={() => seek(position - 15)}>−15s</button>
    <input type="range" aria-label="Playback position" min="0" max={duration || 0} step="0.1" value={position}
      oninput={(e) => seek(Number(e.currentTarget.value))} />
    <button aria-label="Forward 15 seconds" onclick={() => seek(position + 15)}>+15s</button>
    <span>{formatTime(position)} / {formatTime(duration)}</span>
    <label>Speed
      <select bind:value={speed}>
        {#each SPEEDS as s (s)}<option value={s}>{s}×</option>{/each}
      </select>
    </label>
    {#if session.audioExpiresAt !== null}
      <p>{retentionLine(session.audioExpiresAt)}</p>
    {/if}
    {#if lines.length}
      <ol class="lines" aria-label="Timed transcript">
        {#each lines as line, i (i)}
          <li><button aria-current={i === highlighted ? 'true' : undefined} onclick={() => seek(line.startMs / 1000)}>
            <time>{formatTime(line.startMs / 1000)}</time> {line.text}
          </button></li>
        {/each}
      </ol>
    {:else if linesFailed}
      <p role="alert">The timed transcript couldn't load. Playback still works.</p>
    {/if}
  </section>
{:else if view.kind === 'message'}
  <p role="alert">{view.text}</p>
{:else if audioPending(session)}
  <p>Audio will be ready when the transcript is saved.</p>
{/if}

<style>
  .lines { list-style: none; padding: 0; }
  .lines button { all: unset; display: block; cursor: pointer; padding: 2px 4px; border-radius: 4px; }
  .lines button:focus-visible { outline: 2px solid Highlight; outline-offset: 1px; }
  .lines button[aria-current='true'] { background: #8884; font-weight: 600; }
  .lines time { opacity: .7; font-variant-numeric: tabular-nums; }
</style>
