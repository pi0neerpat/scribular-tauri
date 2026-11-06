// @vitest-environment jsdom
import { afterEach, expect, it, vi } from 'vitest';
import { mount, unmount } from 'svelte';

const getUpdateState = vi.fn();
const installUpdate = vi.fn();
const onUpdateState = vi.fn();
const openExternalLink = vi.fn();
vi.mock('../api', () => ({ getUpdateState, installUpdate, onUpdateState, openExternalLink }));
const { default: UpdateBar } = await import('./UpdateBar.svelte');

afterEach(() => vi.clearAllMocks());

it('shows ready idle updates, hides busy states, and Later dismisses only that version', async () => {
  let emit!: (state: unknown) => void;
  onUpdateState.mockImplementation(async (handler) => { emit = handler; return () => {}; });
  getUpdateState.mockResolvedValue({ state: 'ready', version: '0.2.0', notes: '', installable: true });
  const target = document.createElement('div');
  const app = mount(UpdateBar, { target });
  await vi.waitFor(() => expect(target.textContent).toContain('Scribular 0.2.0 is ready'));
  [...target.querySelectorAll('button')].find((button) => button.textContent === 'Later')!.click();
  await vi.waitFor(() => expect(target.textContent).not.toContain('Scribular 0.2.0 is ready'));
  emit({ state: 'ready', version: '0.2.0', notes: '', installable: true });
  await Promise.resolve();
  expect(target.textContent).not.toContain('Scribular 0.2.0 is ready');
  emit({ state: 'ready', version: '0.2.1', notes: '', installable: false });
  await vi.waitFor(() => expect(target.textContent).not.toContain('Scribular'));
  emit({ state: 'ready', version: '0.2.1', notes: '', installable: true });
  await vi.waitFor(() => expect(target.textContent).toContain('Scribular 0.2.1 is ready'));
  unmount(app);
});
