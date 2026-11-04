<script lang="ts">
  // The session's kept audio. Owns the blob URL: revoked when the session
  // changes, the player unmounts or hides, or the audio expires, so the WAV
  // lives only in webview memory and never outlives its retention deadline.
  import type { Session } from '../types/Session';
  import { openAudio, retentionLine, type AudioView } from './session-audio';

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
    };
  });
</script>

{#if view.kind === 'player'}
  <audio controls src={view.url}></audio>
  {#if session.audioExpiresAt !== null}
    <p>{retentionLine(session.audioExpiresAt)}</p>
  {/if}
{:else if view.kind === 'message'}
  <p role="alert">{view.text}</p>
{/if}
