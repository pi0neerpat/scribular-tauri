<script lang="ts">
  import { onMount } from 'svelte';
  import { getShortcutBindings, onShortcutRegistration, setShortcutCapture, setShortcutSettings, shortcutRegistrationState } from '../api';
  import type { ShortcutRegistrationState } from '../types/ShortcutRegistrationState';

  let shortcutState = $state<ShortcutRegistrationState | null>(null);
  let bindings = $state<[string, string][]>([]);
  let error = $state('');
  let capturing = $state(false);
  let held = $state('');
  let busy = $state(false);

  const labels: Record<string, [string, string]> = {
    'shell-new-recording': ['Recording', 'New Recording'], 'shell-record': ['Recording', 'Start/Stop Recording'],
    'shell-pause': ['Recording', 'Pause/Resume Recording'], 'shell-example-style': ['Notes and sessions', 'New Style from Example Note'],
    'shell-copy-note': ['Notes and sessions', 'Copy Note'], 'shell-delete-session': ['Notes and sessions', 'Delete Session'],
    'native-undo': ['Notes and sessions', 'Undo'], 'native-redo': ['Notes and sessions', 'Redo'],
    'shell-find': ['Find and view', 'Find'], 'shell-note': ['Find and view', 'Note'],
    'shell-transcript': ['Find and view', 'Transcript'], 'shell-audio': ['Find and view', 'Audio'],
    'shell-manage-styles': ['Find and view', 'Note Styles'], 'shell-previous': ['Find and view', 'Previous Session'],
    'shell-next': ['Find and view', 'Next Session'], 'shell-sidebar': ['Find and view', 'Show/Hide Sidebar'],
  };
  const groups = ['Recording', 'Notes and sessions', 'Find and view'];

  function display(chord: string | null): string {
    if (!chord) return 'Not set';
    const parts = chord.toLowerCase().split('+');
    const key = parts.at(-1)?.replace(/^key/, '').replace(/^digit/, '') ?? '';
    return `${parts.includes('control') || parts.includes('ctrl') ? '⌃' : ''}${parts.includes('alt') ? '⌥' : ''}${parts.includes('shift') ? '⇧' : ''}${parts.includes('super') || parts.includes('cmdorctrl') ? '⌘' : ''}${key.toUpperCase()}`;
  }

  onMount(() => {
    let eventSeen = false;
    const listener = onShortcutRegistration((current) => { eventSeen = true; if (!capturing && !busy) shortcutState = current; });
    void Promise.all([shortcutRegistrationState(), getShortcutBindings()]).then(([current, fixed]) => { if (!eventSeen) shortcutState = current; bindings = fixed; }).catch((e) => error = String(e));
    return () => { void listener.then((off) => off()); if (capturing) void setShortcutCapture(false); };
  });

  async function change(enabled: boolean, chord = shortcutState?.settings.toggleRecording ?? null) {
    if (!shortcutState || busy) return;
    busy = true;
    try {
      shortcutState = await setShortcutSettings({ globalShortcutsEnabled: enabled, toggleRecording: chord });
      error = '';
      capturing = false;
    } catch (e) {
      error = String(e);
      if (capturing) await endCapture();
    } finally { busy = false; }
  }

  async function beginCapture() {
    if (!shortcutState || busy || capturing) return;
    try { shortcutState = await setShortcutCapture(true); capturing = true; error = ''; }
    catch (e) { error = String(e); }
  }

  async function endCapture() {
    if (!capturing) return;
    capturing = false;
    held = '';
    try { shortcutState = await setShortcutCapture(false); }
    catch (e) { error = String(e); }
  }

  function keydown(event: KeyboardEvent) {
    if (!capturing || busy) return;
    event.preventDefault();
    event.stopPropagation();
    if (event.key === 'Escape') { void endCapture(); return; }
    if (event.key === 'Backspace' || event.key === 'Delete') { void change(shortcutState!.settings.globalShortcutsEnabled, null); return; }
    held = `${event.ctrlKey ? '⌃' : ''}${event.altKey ? '⌥' : ''}${event.shiftKey ? '⇧' : ''}${event.metaKey ? '⌘' : ''}`;
    if (/^(Control|Alt|Shift|Meta)$/.test(event.key)) return;
    const code = event.code;
    if (!/^(Key[A-Z]|Digit[0-9]|F(?:[1-9]|1[0-9]|2[0-4]))$/.test(code)) { error = 'Choose a letter, digit, or F-key.'; void endCapture(); return; }
    const chord = [event.ctrlKey && 'Ctrl', event.altKey && 'Alt', event.shiftKey && 'Shift', event.metaKey && 'Super', code].filter(Boolean).join('+');
    void change(shortcutState!.settings.globalShortcutsEnabled, chord);
  }

  function keyup(event: KeyboardEvent) {
    if (capturing) held = `${event.ctrlKey ? '⌃' : ''}${event.altKey ? '⌥' : ''}${event.shiftKey ? '⇧' : ''}${event.metaKey ? '⌘' : ''}`;
  }
</script>

<section aria-label="Keyboard Shortcuts">
  <h3>Background recording</h3>
  <p>When enabled, this shortcut works in every app. Start recording only with client consent.</p>
  {#if shortcutState}
    <label><input type="checkbox" checked={shortcutState.settings.globalShortcutsEnabled} disabled={busy} onchange={(e) => change(e.currentTarget.checked)} /> Enable background shortcut</label>
    <label for="toggle-recording-shortcut">Start/stop recording</label>
    <button id="toggle-recording-shortcut" class="capture" disabled={busy} onfocus={beginCapture} onkeydown={keydown} onkeyup={keyup} onblur={endCapture}>
      {capturing ? (held || 'Press a shortcut…') : display(shortcutState.settings.toggleRecording)}
    </button>
    <button disabled={busy} onclick={() => change(shortcutState!.settings.globalShortcutsEnabled, 'Ctrl+Alt+Super+R')}>Restore Defaults</button>
    {#if shortcutState.activeChord}<p role="status">Active: {display(shortcutState.activeChord)}</p>{:else if shortcutState.settings.globalShortcutsEnabled}<p role="status">Shortcut inactive</p>{/if}
    {#if shortcutState.error}<p role="alert">{shortcutState.error} <button onclick={() => change(shortcutState!.settings.globalShortcutsEnabled)}>Retry</button></p>{/if}
  {/if}
  {#if error}<p role="alert">{error}</p>{/if}
  <h3>In Scribular</h3>
  {#each groups as group}
    <h4>{group}</h4>
    <dl>
      {#each bindings.filter(([id]) => labels[id]?.[0] === group) as [id, key]}
        <dt>{labels[id][1]}</dt><dd>{display(key.replaceAll('CmdOrCtrl', 'Super'))}</dd>
      {/each}
    </dl>
  {/each}
</section>
