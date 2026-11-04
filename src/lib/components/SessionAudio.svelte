<script lang="ts">
  // The session's kept audio. Owns the blob URL: revoked when the session
  // changes or the player unmounts, so the WAV lives only in webview memory.
  import type { Session } from '../types/Session';
  import { openAudio, retentionLine, type AudioView } from './session-audio';

  let { session }: { session: Session } = $props();

  // Primitive, so a refreshed `session` object with the same id doesn't refetch.
  let id = $derived(session.hasAudio ? session.id : null);
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
