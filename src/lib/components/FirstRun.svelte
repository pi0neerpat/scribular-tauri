<script lang="ts">
  import { onMount } from 'svelte';
  import { downloadModels, getTelemetrySetting, modelsStatus, onModelsProgress, openPrivacySettings, setTelemetrySetting } from '../api';
  import { mergeSnapshot } from '../models';
  import type { ModelProgress } from '../types/ModelProgress';
  import SpeechModelStatus from './SpeechModelStatus.svelte';

  const SPEECH_MODEL = 'whisper-base';
  let { speechReady = $bindable(false) } = $props();
  let models = $state<ModelProgress[]>([]);
  let telemetry = $state<boolean | null>(null);
  let telemetryError = $state<string | null>(null);
  let telemetryChosen = false;
  let speech = $derived(models.find((m) => m.modelId === SPEECH_MODEL));

  function update(p: ModelProgress[]) {
    models = p;
    speechReady = p.some((m) => m.modelId === SPEECH_MODEL && m.state === 'ready');
  }

  async function chooseTelemetry(enabled: boolean) {
    telemetryChosen = true;
    try {
      await setTelemetrySetting(enabled);
      telemetry = enabled;
      telemetryError = null;
    } catch (e) {
      telemetryError = String(e);
    }
  }

  onMount(() => {
    getTelemetrySetting().then((value) => { if (!telemetryChosen) telemetry = value === 'undecided' ? null : value === 'on'; }).catch((e) => (telemetryError = String(e)));
    // Listen before the snapshot and download, so no progress event is missed;
    // entries already updated by an event are newer than the snapshot.
    const unlisten = onModelsProgress((p) => update([...models.filter((m) => m.modelId !== p.modelId), p]));
    unlisten
      .then(() => modelsStatus())
      .then((s) => update(mergeSnapshot(s, models)))
      .finally(downloadModels);
    return () => { unlisten.then((fn) => fn()); };
  });
</script>

{#if !speechReady || telemetry === null}
  <section aria-labelledby={!speechReady ? 'first-run-title' : 'telemetry-title'}>
    {#if !speechReady}
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
    {/if}
    {#if telemetry === null}
      <div aria-labelledby="telemetry-title">
        <h2 id="telemetry-title">Help improve Scribular?</h2>
        <p>Send anonymous usage statistics and error reports. Session content is never sent.</p>
        <button onclick={() => chooseTelemetry(true)}>Allow</button>
        <button onclick={() => chooseTelemetry(false)}>No thanks</button>
      </div>
    {/if}
    {#if telemetryError}<p role="alert">{telemetryError}</p>{/if}
  </section>
{/if}
