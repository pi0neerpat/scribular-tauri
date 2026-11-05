// @vitest-environment jsdom
// AS13: the denied gate offers Enter and Buy and says notes are safe; Buy opens
// the pricing page; a trial warning shows a banner and leaves Start enabled.
import { afterEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, tick, unmount, type Component } from 'svelte';

const openExternalLink = vi.fn(() => Promise.resolve());
vi.mock('../api', async (orig) => ({
  ...(await orig<typeof import('../api')>()),
  openExternalLink,
  currentRecording: vi.fn(() => Promise.resolve(null)),
  onSessionProgress: vi.fn(() => Promise.resolve(() => {})),
}));
const { default: LicenseGate } = await import('./LicenseGate.svelte');
const { default: RecordView } = await import('./RecordView.svelte');
const { accessState } = await import('../license');

const buttons = (t: HTMLElement) => [...t.querySelectorAll('button')];
const named = (t: HTMLElement, text: string) =>
  buttons(t).find((b) => b.textContent?.trim() === text);

let cleanup: (() => void)[] = [];
afterEach(() => {
  cleanup.forEach((f) => f());
  cleanup = [];
  accessState.set(null);
});

function render(c: Component<any>, props: Record<string, unknown>) {
  const target = document.body.appendChild(document.createElement('div'));
  const app = mount(c, { target, props });
  cleanup.push(() => {
    unmount(app);
    target.remove();
  });
  flushSync();
  return target;
}

describe('LicenseGate', () => {
  it('explains a denial with Enter and Buy, and Buy opens pricing', () => {
    const onEnter = vi.fn();
    const t = render(LicenseGate, { access: { kind: 'trialEnded' }, onEnter });
    expect(t.textContent).toContain('Your existing notes are still available.');
    named(t, 'Enter licence')!.click();
    expect(onEnter).toHaveBeenCalled();
    named(t, 'Buy licence')!.click();
    expect(openExternalLink).toHaveBeenCalledWith('https://www.scribular.com/#pricing');
  });

  it('names the specific rejection', () => {
    const t = render(LicenseGate, {
      access: { kind: 'licenseInvalid', reason: 'deviceLimit' },
      onEnter: () => {},
    });
    expect(t.textContent).toContain('maximum number of Macs');
  });
});

describe('LicenseGate warning', () => {
  it('shows an opener failure beside the warning', async () => {
    openExternalLink.mockRejectedValueOnce('no browser' as never);
    const t = render(LicenseGate, {
      access: { kind: 'trialWarning', daysLeft: 2 },
      onEnter: () => {},
    });
    named(t, 'Buy licence')!.click();
    await tick();
    await tick();
    expect(t.querySelector('[role=alert]')?.textContent).toBe('no browser');
  });
});

describe('RecordView with a licence state', () => {
  it('shows the trial warning and keeps Start enabled', async () => {
    accessState.set({ kind: 'trialWarning', daysLeft: 2 });
    const t = render(RecordView, { speechReady: true });
    await tick();
    expect(t.textContent).toContain('Your free trial ends in 2 days.');
    expect(named(t, 'Start recording')!.disabled).toBe(false);
  });

  it('disables Start when denied', async () => {
    accessState.set({ kind: 'licenseLapsed' });
    const t = render(RecordView, { speechReady: true });
    await tick();
    expect(named(t, 'Start recording')!.disabled).toBe(true);
    expect(named(t, 'Buy licence')).toBeDefined();
  });
});
