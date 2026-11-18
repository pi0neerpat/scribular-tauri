// @vitest-environment jsdom
import { expect, it, vi } from 'vitest';
import { mount, unmount } from 'svelte';

const confirmQuit = vi.fn().mockResolvedValue(undefined);
vi.mock('../api', () => ({ confirmQuit }));
const { default: QuitConfirm } = await import('./QuitConfirm.svelte');

it.each([false, true])('supports Escape and confirms the requested exit (restart=%s)', async (restart) => {
  confirmQuit.mockClear();
  const close = vi.fn();
  const target = document.createElement('div');
  document.body.append(target);
  const app = mount(QuitConfirm, { target, props: { restart, close } });
  const dialog = target.querySelector('[role="alertdialog"]')!;
  const confirm = dialog.querySelectorAll('button')[1];
  await vi.waitFor(() => expect(document.activeElement).toBe(dialog));
  dialog.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
  expect(close).toHaveBeenCalledOnce();
  expect(confirmQuit).not.toHaveBeenCalled();
  expect(confirm.textContent).toBe(restart ? 'Restart Now' : 'Quit');
  confirm.click();
  expect(confirmQuit).toHaveBeenCalledExactlyOnceWith(restart);
  unmount(app);
  target.remove();
});
