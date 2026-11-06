// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';
import OnboardingTour from './OnboardingTour.svelte';

vi.mock('../api', () => ({ openExternalLink: vi.fn() }));

describe('onboarding tour', () => {
  let instance: ReturnType<typeof mount> | undefined;
  afterEach(() => {
    if (instance) unmount(instance);
    instance = undefined;
    document.body.innerHTML = '';
  });

  it('does not complete on an outside click, but close and Escape do', () => {
    const target = document.createElement('div');
    document.body.append(target);
    const onComplete = vi.fn();
    instance = mount(OnboardingTour, {
      target,
      props: { onNavigate: vi.fn(), onComplete },
    });
    flushSync();

    document.body.click();
    expect(onComplete).not.toHaveBeenCalled();
    target.querySelector<HTMLButtonElement>('[aria-label="Close tour"]')!.click();
    expect(onComplete).toHaveBeenCalledTimes(1);
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    expect(onComplete).toHaveBeenCalledTimes(2);
  });

  it('drops the sessions step when the app has no session-list anchor', () => {
    const target = document.createElement('div');
    document.body.append(target);
    instance = mount(OnboardingTour, {
      target,
      props: { onNavigate: vi.fn(), onComplete: vi.fn() },
    });
    flushSync();
    expect(target.textContent).toContain('1 of 3');
  });

  it.each([
    [true, 'Everything stays on your device.'],
    [false, 'After you finish a recording, the transcript and summary appear here.'],
  ])('uses the correct sessions-step copy when sessionsEmpty is %s', (sessionsEmpty, expected) => {
    document.body.insertAdjacentHTML('beforeend', '<div data-tour="sessions"></div><div data-tour="input-mode"></div>');
    const target = document.createElement('div');
    document.body.append(target);
    instance = mount(OnboardingTour, {
      target,
      props: { onNavigate: vi.fn(), onComplete: vi.fn(), sessionsEmpty },
    });
    flushSync();
    target.querySelector<HTMLButtonElement>('button:not(.close)')!.click();
    flushSync();
    target.querySelector<HTMLButtonElement>('button:not(.close)')!.click();
    flushSync();
    expect(target.textContent).toContain(expected);
    if (!sessionsEmpty) expect(target.textContent).not.toContain('Everything stays on your device.');
  });
});
