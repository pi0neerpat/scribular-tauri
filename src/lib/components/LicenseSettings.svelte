<script lang="ts">
  // Spec 04 R11/R13/R15: the License section of Settings.
  import { onMount } from 'svelte';
  import { activateLicense, deactivateLicense, getMaskedLicenseKey } from '../api';
  import { accessState, activationText } from '../license';

  let masked = $state<string | null>(null);
  let key = $state('');
  let busy = $state(false);
  let message = $state<string | null>(null);
  let confirming = $state(false);

  let s = $derived($accessState);
  // Lapsed can be only "offline too long", so it isn't called invalid.
  let validity = $derived(
    s?.kind === 'licensed' || s?.kind === 'licensedGrace'
      ? 'Valid'
      : s?.kind === 'licenseLapsed'
        ? 'Not verified'
        : 'Invalid',
  );

  onMount(refresh);

  async function refresh() {
    try {
      masked = await getMaskedLicenseKey();
    } catch (e) {
      message = String(e);
    }
  }

  async function save(e: SubmitEvent) {
    e.preventDefault();
    busy = true;
    message = null;
    try {
      const result = await activateLicense(key);
      message = activationText(result);
      if (result.kind === 'valid') key = '';
      await refresh();
    } catch (err) {
      message = String(err);
    } finally {
      busy = false;
    }
  }

  async function remove() {
    confirming = false;
    message = null;
    try {
      await deactivateLicense();
      await refresh();
    } catch (e) {
      message = String(e);
    }
  }
</script>

<fieldset>
  <legend>License</legend>
  {#if s?.kind === 'trial' || s?.kind === 'trialWarning'}
    <p>Free trial: {s.daysLeft} {s.daysLeft === 1 ? 'day' : 'days'} remaining</p>
  {:else if s?.kind === 'trialEnded'}
    <p>Your free trial has ended. Please purchase a license to continue.</p>
  {/if}

  {#if masked}
    <p>
      <span data-testid="license-validity">{validity}</span>
      <code>{masked}</code>
      {#if confirming}
        Remove the licence from this Mac?
        <button onclick={remove}>Remove</button>
        <button onclick={() => (confirming = false)}>Cancel</button>
      {:else}
        <button onclick={() => (confirming = true)}>Remove</button>
      {/if}
    </p>
  {:else}
    <form onsubmit={save}>
      <label>
        Licence key
        <input bind:value={key} autocomplete="off" spellcheck="false" />
      </label>
      <button type="submit" disabled={busy || !key.trim()}>Save</button>
    </form>
  {/if}
  <p>
    Your licence key and a random device ID are sent to scribular.com to confirm your licence.
    Your recordings and notes never leave this Mac.
  </p>
  {#if message}<p role="status">{message}</p>{/if}
</fieldset>
