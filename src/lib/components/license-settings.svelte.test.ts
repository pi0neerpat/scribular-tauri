// @vitest-environment jsdom
// R11/R13: the Settings licence section: validity badge, trial line, Remove
// asks first, and the typed key is cleared only after a valid activation.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, tick, unmount } from 'svelte';
import type { ActivationResult } from '../types/ActivationResult';

let masked: string | null = null;
const getMaskedLicenseKey = vi.fn(() => Promise.resolve(masked));
const activateLicense = vi.fn<(key: string) => Promise<ActivationResult>>();
const deactivateLicense = vi.fn(() => {
  masked = null;
  return Promise.resolve();
});
vi.mock('../api', async (orig) => ({
  ...(await orig<typeof import('../api')>()),
  getMaskedLicenseKey,
  activateLicense,
  deactivateLicense,
}));
const { default: LicenseSettings } = await import('./LicenseSettings.svelte');
const { accessState } = await import('../license');

const named = (t: HTMLElement, text: string) =>
  [...t.querySelectorAll('button')].find((b) => b.textContent?.trim() === text);
const settle = async () => {
  for (let i = 0; i < 5; i++) await tick();
  flushSync();
};

let cleanup: (() => void)[] = [];
beforeEach(() => {
  masked = null;
  vi.clearAllMocks();
});
afterEach(() => {
  cleanup.forEach((f) => f());
  cleanup = [];
  accessState.set(null);
});

async function render() {
  const target = document.body.appendChild(document.createElement('div'));
  const app = mount(LicenseSettings, { target });
  cleanup.push(() => {
    unmount(app);
    target.remove();
  });
  await settle();
  return target;
}

function type(t: HTMLElement, value: string) {
  const input = t.querySelector('input')!;
  input.value = value;
  input.dispatchEvent(new Event('input'));
  flushSync();
  return input;
}

describe('LicenseSettings', () => {
  it('shows the trial days left', async () => {
    accessState.set({ kind: 'trial', daysLeft: 1 });
    const t = await render();
    expect(t.textContent).toContain('Free trial: 1 day remaining');
  });

  it.each([
    [{ kind: 'licensed', expiresAt: null, verifiedAt: 0 }, 'Valid'],
    [{ kind: 'licensedGrace', daysLeft: 3 }, 'Valid'],
    [{ kind: 'licenseLapsed' }, 'Not verified'],
    [{ kind: 'licenseInvalid', reason: 'invalidKey' }, 'Invalid'],
  ] as const)('labels %o as %s', async (state, label) => {
    masked = 'brav***-042';
    accessState.set(state);
    const t = await render();
    expect(t.querySelector('[data-testid=license-validity]')!.textContent).toBe(label);
  });

  it('asks before removing, and Cancel keeps the key', async () => {
    masked = 'brav***-042';
    const t = await render();
    named(t, 'Remove')!.click();
    flushSync();
    expect(t.textContent).toContain('Remove the licence from this Mac?');
    named(t, 'Cancel')!.click();
    flushSync();
    expect(deactivateLicense).not.toHaveBeenCalled();
    named(t, 'Remove')!.click();
    flushSync();
    named(t, 'Remove')!.click();
    await settle();
    expect(deactivateLicense).toHaveBeenCalledOnce();
    expect(t.querySelector('input')).not.toBeNull();
  });

  it('keeps the typed key on a rejection and clears it when valid', async () => {
    const t = await render();
    activateLicense.mockResolvedValueOnce({ kind: 'invalidKey' });
    type(t, 'brave-quiet-river-042');
    t.querySelector('form')!.requestSubmit();
    await settle();
    expect(t.textContent).toContain("wasn't recognised");
    expect(t.querySelector('input')!.value).toBe('brave-quiet-river-042');

    activateLicense.mockResolvedValueOnce({ kind: 'valid', expiresAt: null });
    t.querySelector('form')!.requestSubmit();
    await settle();
    expect(t.textContent).toContain('Licence activated.');
    expect(t.querySelector('input')!.value).toBe('');
  });
});
