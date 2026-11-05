<script lang="ts">
  // Spec 04 R11: why new recordings are refused, or a non-blocking warning.
  // RecordView disables Start itself; this only explains and offers a way on.
  import { openExternalLink } from '../api';
  import {
    PRICING_URL,
    deniedReason,
    dismissForToday,
    dismissedToday,
    warning,
  } from '../license';
  import type { AccessState } from '../types/AccessState';

  let { access, onEnter }: { access: AccessState | null; onEnter: () => void } = $props();

  let dismissed = $state(dismissedToday());
  let reason = $derived(deniedReason(access));
  let notice = $derived(warning(access));
  let error = $state<string | null>(null);

  function buy() {
    error = null;
    openExternalLink(PRICING_URL).catch((e) => (error = String(e)));
  }

  function dismiss() {
    dismissForToday();
    dismissed = true;
  }
</script>

{#if reason}
  <div role="alert">
    <p>{reason} New recordings need a licence. Your existing notes are still available.</p>
    <button onclick={onEnter}>Enter licence</button>
    <button onclick={buy}>Buy licence</button>
    {#if error}<p>{error}</p>{/if}
  </div>
{:else if notice && !(notice.dismissible && dismissed && dismissedToday())}
  <p role="status">
    {notice.text}
    {#if notice.dismissible}
      <button onclick={buy}>Buy licence</button>
      <button onclick={dismiss}>Dismiss</button>
    {/if}
    {#if error}<span role="alert">{error}</span>{/if}
  </p>
{/if}
