<script lang="ts">
  // What the next recording captures and how it is summarized.
  // Every change saves immediately.
  import { onMount } from 'svelte';
  import {
    downloadModel,
    getRecordingSettings,
    modelsStatus,
    onModelsProgress,
    openPrivacySettings,
    setRecordingPreferences,
    appVersion,
    checkForUpdates,
    getUpdateState,
    installUpdate,
    onUpdateState,
    getAppSettings,
    setAppSettings,
    requestNotificationPermission,
    onSettingsPane,
    getDiagnostics,
    getTelemetrySetting,
    setTelemetrySetting,
    subscribeSummaryAvailability,
    retrySummaryModel,
    viewLogs,
    reportTestError,
    triggerTestPanic,
  } from '../api';
  import { appSettings } from '../app-settings';
  import { startAppearance } from '../appearance-store';
  import AppearancePane from './AppearancePane.svelte';
  import LicenseSettings from './LicenseSettings.svelte';
  import ShortcutsSettings from './ShortcutsSettings.svelte';
  import SummaryUnavailable from './SummaryUnavailable.svelte';
  import type { AppSettings } from '../types/AppSettings';
  import type { Privacy } from '../types/Privacy';
  import type { RecordingSettings } from '../types/RecordingSettings';
  import type { ModelProgress } from '../types/ModelProgress';
  import type { UpdateState } from '../types/UpdateState';
  import type { SummaryAvailability } from '../types/SummaryAvailability';
  import { updateStatus } from './update-status';
  import { navigationLockedBy } from '../navigation';

  let { navigationLocked: lockedProp = false }: { navigationLocked?: boolean } = $props();
  onMount(() => {
    let stop: (() => void) | undefined;
    let disposed = false;
    void startAppearance().then((cleanup) => { if (disposed) cleanup(); else stop = cleanup; });
    return () => { disposed = true; stop?.(); };
  });
  // The Settings webview has no App.svelte, so it tracks model work itself.
  let models = $state<Record<string, ModelProgress>>({});
  let navigationLocked = $derived(lockedProp || navigationLockedBy(Object.values(models)));

  const SPEAKER_MODEL = 'speaker-campplus';

  let settings = $state<RecordingSettings | null>(null);
  let error = $state<string | null>(null);
  let denied = $state<Privacy | null>(null);
  let speakerModel = $state<ModelProgress | null>(null);
  let version = $state<string>('');
  let update = $state<UpdateState>({ state: 'idle', installable: false });
  let persistedAppSettings = $state<AppSettings | null>(null);
  let appSettingsSaving = $state(false);
  type Category = 'General' | 'Appearance' | 'Recording' | 'Keyboard Shortcuts' | 'License';
  let category = $state<Category>((new URLSearchParams(location.search).get('pane') as Category) || 'General');
  let telemetryOn = $state(false);
  let telemetryKnown = $state(false);
  let telemetrySaving = $state(false);
  let diagnosticsStatus = $state<string | null>(null);
  let summaryAvailability = $state<SummaryAvailability>('downloading');
  let summaryAvailabilityError = $state(false);
  let summaryAvailabilityRetrying = $state(false);
  let retrySummaryAvailability = $state<(() => void) | null>(null);

  function chooseCategory(next: typeof category) {
    if (navigationLocked) return;
    if (next === category) return;
    category = next;
  }

  onMount(() => {
    const paneListener = onSettingsPane((pane) => {
      if (['General', 'Appearance', 'Recording', 'Keyboard Shortcuts', 'License'].includes(pane)) category = pane as Category;
    });
    let unlisten: (() => void) | undefined;
    let updateUnlisten: (() => void) | undefined;
    let summaryUnlisten: (() => void) | undefined;
    let disposed = false;
    let summaryAttempt = 0;
    let progressVersion = 0;
    let sawUpdate = false;
    const subscribeSummary = async () => {
      const attempt = ++summaryAttempt;
      summaryAvailabilityRetrying = true;
      summaryUnlisten?.();
      summaryUnlisten = undefined;
      try {
        const remove = await subscribeSummaryAvailability((state) => {
          if (attempt !== summaryAttempt) return;
          summaryAvailability = state;
          summaryAvailabilityError = false;
        }, () => { if (!disposed && attempt === summaryAttempt) summaryAvailabilityError = true; });
        if (disposed || attempt !== summaryAttempt) remove(); else summaryUnlisten = remove;
      } catch {
        if (!disposed && attempt === summaryAttempt) summaryAvailabilityError = true;
      } finally {
        if (attempt === summaryAttempt) summaryAvailabilityRetrying = false;
      }
    };
    retrySummaryAvailability = subscribeSummary;
    void (async () => {
      subscribeSummary();
      try {
        const app = await getAppSettings();
        appSettings.set(app);
        persistedAppSettings = app;
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
        telemetryOn = (await getTelemetrySetting()) === 'on';
        telemetryKnown = true;
      } catch {
        diagnosticsStatus = 'Could not read telemetry setting';
      }
      try {
        try {
          const removeListener = await onModelsProgress((p) => {
            models[p.modelId] = p;
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
        for (const m of status) models[m.modelId] ??= m;
        if (version === progressVersion) speakerModel = status.find((m) => m.modelId === SPEAKER_MODEL) ?? null;
      } catch (e) {
        error = String(e);
      }
    })();
    return () => { disposed = true; summaryAttempt++; unlisten?.(); updateUnlisten?.(); summaryUnlisten?.(); paneListener.then((fn) => fn()); retrySummaryAvailability = null; };
  });

  async function save() {
    if (!settings) return;
    try {
      await setRecordingPreferences(settings.keepAudio, settings.multiSpeaker);
      error = null;
    } catch (e) {
      error = String(e);
    }
  }

  async function saveAppSettings(key: keyof AppSettings, enabled: boolean) {
    if (!persistedAppSettings || appSettingsSaving) return;
    const next = { ...persistedAppSettings, [key]: enabled };
    appSettingsSaving = true;
    appSettings.set(next);
    error = null;
    try {
      if (next.notifySummaryComplete && !persistedAppSettings.notifySummaryComplete) {
        if (!(await requestNotificationPermission())) {
          next.notifySummaryComplete = false;
          denied = 'notifications';
        } else {
          denied = null;
        }
      }
      await setAppSettings(next);
      persistedAppSettings = next;
      appSettings.set(next);
    } catch (e) {
      error = String(e);
      appSettings.set(persistedAppSettings);
    } finally {
      appSettingsSaving = false;
    }
  }

  async function changeTelemetry() {
    if (telemetrySaving) return;
    telemetrySaving = true;
    try {
      await setTelemetrySetting(telemetryOn);
      telemetryKnown = true;
      diagnosticsStatus = null;
    } catch (e) {
      const setting = await getTelemetrySetting().catch(() => 'unknown');
      telemetryKnown = setting !== 'unknown';
      telemetryOn = setting === 'on';
      diagnosticsStatus = String(e);
    } finally {
      telemetrySaving = false;
    }
  }

  async function copyDiagnostics() {
    try {
      await navigator.clipboard.writeText(await getDiagnostics());
      diagnosticsStatus = 'Diagnostics copied';
    } catch { diagnosticsStatus = 'Failed to copy diagnostics'; }
  }

  async function retrySummary() {
    try { await retrySummaryModel(); }
    catch (e) { error = String(e); }
  }

  async function openLogs() {
    try { await viewLogs(); diagnosticsStatus = null; }
    catch { diagnosticsStatus = 'Failed to open logs'; }
  }
</script>

<section>
  <h2>Settings</h2>
  <nav aria-label="Settings categories">
    {#each ['General', 'Appearance', 'Recording', 'Keyboard Shortcuts', 'License'] as item}
      <button aria-pressed={category === item} disabled={navigationLocked} onclick={() => chooseCategory(item as typeof category)}>{item}</button>
    {/each}
  </nav>
  {#if summaryAvailability === 'unsupported' || summaryAvailability === 'loadFailed'}
    <SummaryUnavailable loadFailed={summaryAvailability === 'loadFailed'} onRetry={retrySummary} />
  {/if}
  {#if summaryAvailabilityError}
    <p role="alert">Couldn't check summary availability.</p>
    <button disabled={summaryAvailabilityRetrying} onclick={() => retrySummaryAvailability?.()}>Check again</button>
  {/if}
  {#if settings}
  {#if category === 'Recording'}
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

  {/if}

  {#if category === 'General'}
    <fieldset>
      <legend>Updates</legend>
      <label><input type="checkbox" checked={$appSettings.checkForUpdatesAutomatically} disabled={!persistedAppSettings || appSettingsSaving} onchange={(e) => saveAppSettings('checkForUpdatesAutomatically', e.currentTarget.checked)} /> Check for updates automatically</label>
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

  {#if category === 'General'}
    <fieldset>
      <legend>General</legend>
      <label><input type="checkbox" checked={$appSettings.notifySummaryComplete} disabled={!persistedAppSettings || appSettingsSaving} onchange={(e) => saveAppSettings('notifySummaryComplete', e.currentTarget.checked)} /> Show summarization complete alert</label>
      {#if denied === 'notifications'}
        <p role="status">Notifications are disabled. Allow Scribular in System Settings to show summary alerts.
          <button onclick={() => openPrivacySettings('notifications')}>Open System Settings</button>
        </p>
      {/if}
    </fieldset>
  {/if}
  {#if category === 'Appearance'}
    <AppearancePane />
  {/if}

  {#if category === 'Recording'}
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

  {#if category === 'License'}<LicenseSettings />{/if}
  {#if category === 'Keyboard Shortcuts'}<ShortcutsSettings />{/if}

  {#if category === 'General'}
  <fieldset>
    <legend>Privacy &amp; diagnostics</legend>
    <label><input type="checkbox" bind:checked={telemetryOn} disabled={!telemetryKnown || telemetrySaving} onchange={changeTelemetry} /> Send usage and error reports</label>
    <button onclick={copyDiagnostics}>Copy diagnostics</button>
    <button onclick={openLogs}>View logs</button>
    {#if diagnosticsStatus}<p role="status">{diagnosticsStatus}</p>{/if}
  </fieldset>
  {#if import.meta.env.DEV}
    <fieldset>
      <legend>Developer</legend>
      <button onclick={() => reportTestError().catch(() => (diagnosticsStatus = 'Failed to send test error'))}>Send test error</button>
      <button onclick={() => triggerTestPanic().catch(() => (diagnosticsStatus = 'Failed to trigger test panic'))}>Trigger test panic</button>
    </fieldset>
  {/if}
  {/if}

  {#if denied && denied !== 'notifications'}
    <p role="alert">
      Scribular needs permission to record app audio.
      <button onclick={() => openPrivacySettings(denied!)}>Open System Settings</button>
    </p>
  {/if}
  {#if denied === 'notifications' && category !== 'General'}
    <p role="status">
      Notifications are disabled. Allow them for Scribular in System Settings.
      <button onclick={() => openPrivacySettings('notifications').catch((e) => (error = String(e)))}>Open System Settings</button>
    </p>
  {/if}
  {#if error}
    <p role="alert">{error}</p>
  {/if}
</section>
