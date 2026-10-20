<script lang="ts">
  // What the next recording captures and which prompt summarizes it.
  // Every change saves immediately.
  import { onMount } from 'svelte';
  import {
    deniedPrivacy,
    getRecordingSettings,
    listAudioApps,
    listPrompts,
    openPrivacySettings,
    setRecordingSettings,
  } from '../api';
  import type { AudioApp } from '../types/AudioApp';
  import type { InputMode } from '../types/InputMode';
  import type { Privacy } from '../types/Privacy';
  import type { Prompt } from '../types/Prompt';
  import type { RecordingSettings } from '../types/RecordingSettings';

  const modes: [InputMode, string][] = [
    ['mic', 'Microphone'],
    ['system', 'App audio'],
    ['micAndSystem', 'Microphone and app audio'],
  ];

  let settings = $state<RecordingSettings | null>(null);
  let prompts = $state<Prompt[]>([]);
  let apps = $state<AudioApp[]>([]);
  let error = $state<string | null>(null);
  let denied = $state<Privacy | null>(null);

  // The saved app stays selectable while it isn't playing.
  let appNames = $derived.by(() => {
    const names = apps.map((a) => a.name);
    const saved = settings?.targetApp;
    return saved && !names.includes(saved) ? [saved, ...names] : names;
  });
  let selectedIcon = $derived(apps.find((a) => a.name === settings?.targetApp)?.icon ?? null);

  onMount(async () => {
    try {
      [settings, prompts] = await Promise.all([getRecordingSettings(), listPrompts()]);
    } catch (e) {
      error = String(e);
    }
    await refreshApps();
  });

  async function refreshApps() {
    try {
      apps = await listAudioApps();
      denied = null;
    } catch (e) {
      denied = deniedPrivacy(String(e));
      if (!denied) error = String(e);
    }
  }

  async function save() {
    if (!settings) return;
    try {
      await setRecordingSettings(settings);
      error = null;
    } catch (e) {
      error = String(e);
    }
  }
</script>

<section>
  <h2>Settings</h2>
  {#if settings}
    <fieldset>
      <legend>Record</legend>
      {#each modes as [mode, label] (mode)}
        <label>
          <input type="radio" bind:group={settings.inputMode} value={mode} onchange={save} />
          {label}
        </label>
      {/each}
    </fieldset>

    {#if settings.inputMode !== 'mic'}
      <label>
        App
        {#if selectedIcon}
          <img src="data:image/png;base64,{selectedIcon}" alt="" width="16" height="16" />
        {/if}
        <select bind:value={settings.targetApp} onchange={save}>
          <option value={null}>Choose an app…</option>
          {#each appNames as name (name)}
            <option value={name}>{name}</option>
          {/each}
        </select>
      </label>
      <button onclick={refreshApps}>Refresh</button>
      <p>Only apps playing audio right now are listed.</p>
    {/if}

    <label>
      Summary prompt
      <select bind:value={settings.promptId} onchange={save}>
        {#each prompts as prompt (prompt.id)}
          <option value={prompt.id}>{prompt.name}</option>
        {/each}
      </select>
    </label>
  {/if}

  {#if denied}
    <p role="alert">
      Scribular needs permission to record app audio.
      <button onclick={() => openPrivacySettings(denied!)}>Open System Settings</button>
    </p>
  {/if}
  {#if error}
    <p role="alert">{error}</p>
  {/if}
</section>
