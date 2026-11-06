<script lang="ts">
  // What the next recording captures and how it is summarized.
  // Every change saves immediately.
  import { onMount } from 'svelte';
  import {
    deniedPrivacy,
    downloadModel,
    getRecordingSettings,
    listAudioApps,
    modelsStatus,
    onModelsProgress,
    openPrivacySettings,
    setRecordingSettings,
    appVersion,
    checkForUpdates,
    getUpdateState,
    installUpdate,
    onUpdateState,
    getAppSettings,
    setAppSettings,
    requestNotificationPermission,
    openExternalLink,
  } from '../api';
  import { appSettings } from '../app-settings';
  import LicenseSettings from './LicenseSettings.svelte';
  import NoteStyles from './NoteStyles.svelte';
  import SummaryModel from './SummaryModel.svelte';
  import type { AudioApp } from '../types/AudioApp';
  import type { InputMode } from '../types/InputMode';
  import type { Privacy } from '../types/Privacy';
  import type { RecordingSettings } from '../types/RecordingSettings';
  import type { ModelProgress } from '../types/ModelProgress';
  import type { UpdateState } from '../types/UpdateState';
  import { updateStatus } from './update-status';

  let { navigationLocked = false }: { navigationLocked?: boolean } = $props();

  const SPEAKER_MODEL = 'speaker-campplus';

  const modes: [InputMode, string][] = [
    ['mic', 'Microphone'],
    ['system', 'App audio'],
    ['micAndSystem', 'Microphone and app audio'],
  ];

  let settings = $state<RecordingSettings | null>(null);
  let apps = $state<AudioApp[]>([]);
  let error = $state<string | null>(null);
  let denied = $state<Privacy | null>(null);
  let speakerModel = $state<ModelProgress | null>(null);
  let version = $state<string>('');
  let update = $state<UpdateState>({ state: 'idle', installable: false });
  let notifyWasEnabled = $state(false);
  let category = $state<'Note styles' | 'Session' | 'General' | 'About' | 'License'>('Session');
  let noteStyles = $state<{ requestLeave: (action: () => void, opener?: HTMLElement) => void } | undefined>();

  export function requestLeave(action: () => void, opener?: HTMLElement) {
    if (noteStyles) noteStyles.requestLeave(action, opener);
    else action();
  }

  function chooseCategory(next: typeof category, opener: HTMLElement) {
    if (navigationLocked) return;
    if (next === category) return;
    requestLeave(() => (category = next), opener);
  }

  // The saved app stays selectable while it isn't playing.
  let appNames = $derived.by(() => {
    const names = apps.map((a) => a.name);
    const saved = settings?.targetApp;
    return saved && !names.includes(saved) ? [saved, ...names] : names;
  });
  let selectedIcon = $derived(apps.find((a) => a.name === settings?.targetApp)?.icon ?? null);

  onMount(() => {
    let unlisten: (() => void) | undefined;
    let updateUnlisten: (() => void) | undefined;
    let disposed = false;
    let progressVersion = 0;
    let sawUpdate = false;
    void (async () => {
      try {
        const app = await getAppSettings();
        appSettings.set(app);
        notifyWasEnabled = app.notifySummaryComplete;
      } catch (e) { error = String(e); }
      try {
        const remove = await onUpdateState((state) => { sawUpdate = true; update = state; });
        if (disposed) remove(); else updateUnlisten = remove;
      } catch { /* The snapshot below still supplies the update state. */ }
      try {
        const [app, current] = await Promise.all([appVersion(), getUpdateState()]);
        version = app.version;
        // An event that arrived meanwhile is newer than the snapshot.
        if (!sawUpdate) update = current;
      } catch { /* Update status is optional while the rest of Settings loads. */ }
      try {
        try {
          const removeListener = await onModelsProgress((p) => {
            if (p.modelId === SPEAKER_MODEL) {
              progressVersion++;
              speakerModel = p;
            }
          });
          if (disposed) removeListener();
          else unlisten = removeListener;
        } catch { /* The status snapshot still supplies model state. */ }
        settings = await getRecordingSettings();
        const version = progressVersion;
        const status = await modelsStatus();
        if (version === progressVersion) speakerModel = status.find((m) => m.modelId === SPEAKER_MODEL) ?? null;
      } catch (e) {
        error = String(e);
      }
      if (settings && settings.inputMode !== 'mic') await refreshApps();
    })();
    return () => { disposed = true; unlisten?.(); updateUnlisten?.(); };
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

  // Listing apps runs AudioCapCLI, so only when the picker shows.
  async function changeMode() {
    await save();
    if (settings && settings.inputMode !== 'mic') await refreshApps();
    else denied = null;
  }

  async function save() {
    if (!settings) return;
    try {
      // This settings save leaves the active style unchanged.
      await setRecordingSettings({ ...settings, promptId: null });
      error = null;
    } catch (e) {
      error = String(e);
    }
  }

  async function saveAppSettings() {
    const next = $appSettings;
    try {
      denied = null;
      if (next.notifySummaryComplete && !notifyWasEnabled) {
        if (!(await requestNotificationPermission())) {
          appSettings.update((s) => ({ ...s, notifySummaryComplete: false }));
          denied = 'notifications';
        } else {
          notifyWasEnabled = true;
        }
      }
      if (!next.notifySummaryComplete) notifyWasEnabled = false;
      await setAppSettings($appSettings);
    } catch (e) {
      error = String(e);
      appSettings.set(await getAppSettings());
    }
  }
</script>

<section>
  <h2>Settings</h2>
  <nav aria-label="Settings categories">
    {#each ['Note styles', 'Session', 'General', 'About', 'License'] as item}
      <button aria-pressed={category === item} disabled={navigationLocked} onclick={(e) => chooseCategory(item as typeof category, e.currentTarget)}>{item}</button>
    {/each}
  </nav>
  {#if settings}
  {#if category === 'Session'}
    <fieldset>
      <legend>Record</legend>
      {#each modes as [mode, label] (mode)}
        <label>
          <input type="radio" bind:group={settings.inputMode} value={mode} onchange={changeMode} />
          {label}
        </label>
      {/each}
    </fieldset>
  {/if}

  {#if category === 'Session'}
    <fieldset>
      <legend>Multi-speaker</legend>
      <p>Identify different speakers after recording. The speaker model downloads once and stays on this Mac.</p>
      {#if speakerModel?.state === 'ready'}
        <label><input type="checkbox" bind:checked={settings.multiSpeaker} onchange={save} /> Identify speakers</label>
      {:else}
        <button onclick={() => downloadModel(SPEAKER_MODEL)} disabled={speakerModel?.state === 'downloading' || speakerModel?.state === 'verifying'}>
          {speakerModel?.state === 'downloading' || speakerModel?.state === 'verifying' ? 'Downloading speaker model…' : 'Download speaker model'}
        </button>
        {#if speakerModel?.state === 'downloading'}
          <progress aria-label="Speaker model download progress" value={speakerModel.downloadedBytes} max={speakerModel.totalBytes}></progress>
        {/if}
        {#if speakerModel?.error}<p role="alert">{speakerModel.error}</p>{/if}
      {/if}
    </fieldset>

    <fieldset>
      <legend>Updates</legend>
      <p>Current version: {version || 'Loading…'}</p>
      <p role="status">{updateStatus(update)}</p>
      <button
        onclick={() => checkForUpdates().catch((e) => (error = String(e)))}
        disabled={update.state === 'checking' || update.state === 'downloading'}
      >Check for updates</button>
      {#if update.state === 'ready'}
        <button
          onclick={() => installUpdate().catch((e) => (error = String(e)))}
          disabled={!update.installable}
          title={!update.installable ? 'Finish your recording or summary first' : undefined}
        >Restart to update</button>
        {#if !update.installable}<p>Finish your recording or summary first</p>{/if}
      {/if}
    </fieldset>
  {/if}

  {#if category === 'Session'}
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
  {/if}

  {#if category === 'General'}
    <fieldset>
      <legend>General</legend>
      <label><input type="checkbox" bind:checked={$appSettings.showBackgroundAnimation} onchange={saveAppSettings} /> Show background animation</label>
      <label><input type="checkbox" bind:checked={$appSettings.notifySummaryComplete} onchange={saveAppSettings} /> Show summarization complete alert</label>
      {#if denied === 'notifications'}
        <p role="status">Notifications are disabled. Allow Scribular in System Settings to show summary alerts.
          <button onclick={() => openPrivacySettings('notifications')}>Open System Settings</button>
        </p>
      {/if}
    </fieldset>
  {/if}

  {#if category === 'Session'}
    <fieldset>
      <legend>Audio recording</legend>
      <p>
        Save session audio. Recordings are stored encrypted on this Mac and deleted automatically
        7 days after the session. Off by default. Turn on only if you need to check a transcript
        against the recording.
      </p>
      <label>
        <input type="radio" bind:group={settings.keepAudio} value={false} onchange={save} />
        Off
      </label>
      <label>
        <input type="radio" bind:group={settings.keepAudio} value={true} onchange={save} />
        On
      </label>
      {#if settings.keepAudio}
        <p>Recordings are removed with the session if you delete it.</p>
      {:else}
        <p>Existing recordings are deleted on their own schedule.</p>
      {/if}
    </fieldset>
  {/if}
  {/if}

  {#if category === 'Note styles'}
    <NoteStyles bind:this={noteStyles} />
    <SummaryModel />
  {/if}

  {#if category === 'License'}<LicenseSettings />{/if}
  {#if category === 'About'}
    <p>Version: {version || 'Loading…'}</p>
    <button onclick={() => openExternalLink('mailto:support@scribular.com').catch((e) => (error = String(e)))}>Contact support</button>
    <button onclick={() => openExternalLink('https://www.scribular.com').catch((e) => (error = String(e)))}>Scribular website</button>
  {/if}

  {#if denied && denied !== 'notifications'}
    <p role="alert">
      Scribular needs permission to record app audio.
      <button onclick={() => openPrivacySettings(denied!)}>Open System Settings</button>
    </p>
  {/if}
  {#if denied === 'notifications'}
    <p role="status">
      Notifications are disabled. Allow them for Scribular in System Settings.
      <button onclick={() => openPrivacySettings('notifications').catch((e) => (error = String(e)))}>Open System Settings</button>
    </p>
  {/if}
  {#if error}
    <p role="alert">{error}</p>
  {/if}
</section>
