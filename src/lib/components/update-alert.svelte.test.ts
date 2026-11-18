// @vitest-environment jsdom
import { afterEach, expect, it, vi } from 'vitest';
import { mount, unmount } from 'svelte';

const getUpdateState = vi.fn();
const installUpdate = vi.fn();
const onUpdateState = vi.fn();
const showUpdateReadyAlert = vi.fn();
vi.mock('../api', () => ({ getUpdateState, installUpdate, onUpdateState, showUpdateReadyAlert }));
const { default: UpdateAlert } = await import('./UpdateAlert.svelte');

afterEach(() => vi.clearAllMocks());
const ready = (version = '0.2.0') => ({ state: 'ready', version, notes: '', installable: true });

it('defers a ready update through recording and prompts once per version after Stop', async () => {
  let emit!: (state: unknown) => void;
  onUpdateState.mockImplementation(async (handler) => { emit = handler; return () => {}; });
  getUpdateState.mockResolvedValue(ready());
  showUpdateReadyAlert.mockResolvedValue(false);
  const props = $state({ recording: true });
  const target = document.createElement('div');
  const app = mount(UpdateAlert, { target, props });
  await vi.waitFor(() => expect(emit).toBeDefined());
  emit(ready());
  await Promise.resolve();
  expect(showUpdateReadyAlert).not.toHaveBeenCalled();
  props.recording = false;
  await vi.waitFor(() => expect(showUpdateReadyAlert).toHaveBeenCalledExactlyOnceWith('0.2.0'));
  emit(ready());
  await Promise.resolve();
  expect(showUpdateReadyAlert).toHaveBeenCalledTimes(1);
  emit(ready('0.2.1'));
  await vi.waitFor(() => expect(showUpdateReadyAlert).toHaveBeenCalledTimes(2));
  expect(installUpdate).not.toHaveBeenCalled();
  unmount(app);
});

it('restarts only after Restart Now and shows a recoverable error on alert failure', async () => {
  let emit!: (state: unknown) => void;
  onUpdateState.mockImplementation(async (handler) => { emit = handler; return () => {}; });
  getUpdateState.mockResolvedValue({ state: 'idle', installable: false });
  showUpdateReadyAlert.mockResolvedValueOnce(true).mockRejectedValueOnce('dialog failed');
  installUpdate.mockResolvedValue(undefined);
  const target = document.createElement('div');
  const app = mount(UpdateAlert, { target });
  await vi.waitFor(() => expect(emit).toBeDefined());
  emit({ ...ready(), installable: false });
  await vi.waitFor(() => expect(installUpdate).toHaveBeenCalledOnce());
  emit(ready('0.2.1'));
  await vi.waitFor(() => expect(target.querySelector('[role="alert"]')?.textContent).toContain('Settings → General'));
  expect(installUpdate).toHaveBeenCalledTimes(1);
  unmount(app);
});
