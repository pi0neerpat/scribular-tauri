<script lang="ts">
  import { onMount } from 'svelte';
  import { downloadModels, modelsStatus, onModelsProgress, openPrivacySettings } from '../api';
  import type { ModelProgress } from '../types/ModelProgress';

  const SPEECH_MODEL = 'whisper-base';
  let { speechReady = $bindable(false) } = $props();
  let models = $state<ModelProgress[]>([]);

  function update(p: ModelProgress[]) {
    models = p;
    speechReady = p.some((m) => m.modelId === SPEECH_MODEL && m.state === 'ready');
  }

  onMount(() => {
    modelsStatus().then(update);
    const unlisten = onModelsProgress((p) => update([...models.filter((m) => m.modelId !== p.modelId), p]));
    downloadModels();
    return () => { unlisten.then((fn) => fn()); };
  });
</script>

{#if !speechReady}
  <section aria-labelledby="first-run-title">
    <h2 id="first-run-title">Getting Scribular ready</h2>
    <ol>
      <li>
        Allow microphone access when you start your first recording.
        <button onclick={() => openPrivacySettings('microphone')}>Open Microphone Settings</button>
      </li>
      <li>
        Allow Screen &amp; System Audio Recording when prompted for app audio.
        <button onclick={() => openPrivacySettings('systemAudio')}>Open System Audio Settings</button>
      </li>
      <li>Downloading the speech model{#if models.find((m) => m.modelId === SPEECH_MODEL)} ({Math.floor((models.find((m) => m.modelId === SPEECH_MODEL)!.downloadedBytes / Math.max(1, models.find((m) => m.modelId === SPEECH_MODEL)!.totalBytes)) * 100)}%){/if}.</li>
    </ol>
    <p>Recording becomes available when the speech model is ready.</p>
  </section>
{/if}
