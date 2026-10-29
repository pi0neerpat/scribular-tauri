<script lang="ts">
  import { onMount } from 'svelte';
  import { downloadModels, modelsStatus, onModelsProgress, openPrivacySettings } from '../api';
  import type { ModelProgress } from '../types/ModelProgress';
  import SpeechModelStatus from './SpeechModelStatus.svelte';

  const SPEECH_MODEL = 'whisper-base';
  let { speechReady = $bindable(false) } = $props();
  let models = $state<ModelProgress[]>([]);
  let speech = $derived(models.find((m) => m.modelId === SPEECH_MODEL));

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
      <SpeechModelStatus {speech} />
    </ol>
    <p>Recording becomes available when the speech model is ready.</p>
  </section>
{/if}
