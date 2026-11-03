<script lang="ts">
  import { openPrivacySettings } from '../api';
  import type { Privacy } from '../types/Privacy';

  let { pane }: { pane: Privacy } = $props();
</script>

<p role="alert">
  {pane === 'microphone' ? 'Microphone' : 'App audio'} access is off.
  <button onclick={() => openPrivacySettings(pane)}>Open System Settings</button>
</p>
