<script lang="ts">
  import { openPrivacySettings } from '../api';
  import type { Privacy } from '../types/Privacy';

  let { pane }: { pane: Privacy } = $props();
</script>

<p role="alert">
  {pane === 'microphone' ? 'Microphone access is off.' : 'App audio permission is required or unconfirmed.'}
  <button onclick={() => openPrivacySettings(pane)}>{pane === 'systemAudio' ? 'Reopen System Audio Settings' : 'Open System Settings'}</button>
</p>
