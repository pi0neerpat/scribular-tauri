// @vitest-environment jsdom
// Parity spec 03 AS10: the discard dialog guards a dirty editor.
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';

const prompt = (id: string, name: string, isDefault = false) =>
  ({ id, name, description: '', text: `${name} text`, isDefault, starred: true });
const list: { prompts: ReturnType<typeof prompt>[]; activeId: string; createdId: string | null } = {
  prompts: [prompt('a', 'Alpha'), prompt('b', 'Beta')], activeId: 'a', createdId: null,
};

const getPromptList = vi.fn();
const updatePrompt = vi.fn();
const createPrompt = vi.fn();
const deletePrompt = vi.fn();
const onCloseRequested = vi.fn();
const destroyWindow = vi.fn();
let closeHandler: ((event: { preventDefault: () => void }) => void) | undefined;
vi.mock('../api', () => ({
  getPromptList, updatePrompt, createPrompt, deletePrompt, duplicatePrompt: vi.fn(),
  setActivePrompt: vi.fn(), setPromptStarred: vi.fn(),
}));
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ onCloseRequested, destroy: destroyWindow }),
}));
const { default: NoteStyles } = await import('./NoteStyles.svelte');

let target: HTMLElement;
let app: ReturnType<typeof mount>;

const button = (label: string) =>
  [...target.querySelectorAll('button')].find((b) => b.textContent?.trim() === label)!;
const input = () => target.querySelector<HTMLInputElement>('li input')!;
const dialog = () => target.querySelector('[role="dialog"]');

beforeEach(async () => {
  onCloseRequested.mockImplementation((handler) => { closeHandler = handler; return Promise.resolve(vi.fn()); });
  getPromptList.mockResolvedValue(list);
  target = document.createElement('div');
  document.body.append(target);
  app = mount(NoteStyles, { target });
  await vi.waitFor(() => expect(button('Alpha')).toBeTruthy());
  button('Alpha').click();
  flushSync();
});

afterEach(() => { unmount(app); target.remove(); vi.clearAllMocks(); });

function typeName(value: string) {
  input().value = value;
  input().dispatchEvent(new Event('input'));
  flushSync();
}

it('opens another style without a dialog when nothing changed', () => {
  button('Beta').click();
  flushSync();
  expect(dialog()).toBeNull();
  expect(input().value).toBe('Beta');
});

it('cancel keeps the draft open', () => {
  typeName('Alpha 2');
  expect(target.textContent).toContain('Unsaved changes');
  button('Beta').click();
  flushSync();
  expect(dialog()?.textContent).toContain('Discard changes?');
  button('Cancel').click();
  flushSync();
  expect(dialog()).toBeNull();
  expect(input().value).toBe('Alpha 2');
});

it('discard drops the edits and opens the other style', () => {
  typeName('Alpha 2');
  button('Beta').click();
  flushSync();
  button('Discard').click();
  flushSync();
  expect(input().value).toBe('Beta');
  expect(updatePrompt).not.toHaveBeenCalled();
});

it('save keeps the edits, then opens the other style', async () => {
  updatePrompt.mockResolvedValue({ ...list, prompts: [{ ...list.prompts[0], name: 'Alpha 2' }, list.prompts[1]] });
  typeName('Alpha 2');
  button('Beta').click();
  flushSync();
  dialog()!.querySelector<HTMLButtonElement>('button')!.click();
  await vi.waitFor(() => expect(input().value).toBe('Beta'));
  expect(updatePrompt).toHaveBeenCalledWith('a', 'Alpha 2', 'Alpha text');
  expect(target.textContent).toContain('Alpha 2');
});

it('asks before deleting a dirty style', () => {
  typeName('Alpha 2');
  button('Delete').click();
  flushSync();
  expect(dialog()?.textContent).toContain('Discard changes?');
  expect(deletePrompt).not.toHaveBeenCalled();
});

it('focuses the discard dialog and lets Escape cancel it', async () => {
  typeName('Alpha 2');
  button('Beta').click();
  flushSync();
  await vi.waitFor(() => expect(dialog()?.contains(document.activeElement)).toBe(true));
  dialog()!.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
  flushSync();
  expect(dialog()).toBeNull();
  expect(input().value).toBe('Alpha 2');
});

it('focuses Cancel in the delete dialog', async () => {
  const opener = button('Alpha');
  button('Delete').click();
  flushSync();
  await vi.waitFor(() => expect(dialog()?.contains(document.activeElement)).toBe(true));
  expect(document.activeElement?.textContent).toBe('Cancel');
  button('Cancel').click();
  expect(opener.isConnected).toBe(true);
  await vi.waitFor(() => expect(document.activeElement).toBe(opener), { timeout: 3000 });
});

it('traps focus in the discard dialog and restores its opener on close', async () => {
  typeName('Alpha 2');
  const opener = button('Beta');
  opener.click();
  flushSync();
  await vi.waitFor(() => expect(dialog()?.contains(document.activeElement)).toBe(true));

  const buttons = dialog()!.querySelectorAll<HTMLButtonElement>('button');
  buttons[2].focus();
  const tabAllowed = buttons[2].dispatchEvent(new KeyboardEvent('keydown', {
    key: 'Tab', bubbles: true, cancelable: true,
  }));
  expect(tabAllowed).toBe(false);
  expect(document.activeElement).toBe(buttons[0]);

  buttons[0].focus();
  buttons[0].dispatchEvent(new KeyboardEvent('keydown', {
    key: 'Tab', shiftKey: true, bubbles: true, cancelable: true,
  }));
  expect(document.activeElement).toBe(buttons[2]);

  dialog()!.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
  flushSync();
  await vi.waitFor(() => expect(document.activeElement).toBe(opener));
});

it('asks before closing the window with a dirty style', async () => {
  typeName('Alpha 2');
  await vi.waitFor(() => expect(onCloseRequested).toHaveBeenCalledOnce());
  const event = { preventDefault: vi.fn() };
  closeHandler!(event);
  flushSync();
  expect(event.preventDefault).toHaveBeenCalledOnce();
  expect(dialog()?.textContent).toContain('Discard changes?');
  button('Cancel').click();
  flushSync();
  expect(destroyWindow).not.toHaveBeenCalled();
  closeHandler!(event);
  flushSync();
  button('Discard').click();
  expect(destroyWindow).toHaveBeenCalledOnce();
});

it('saves a dirty style before closing the window', async () => {
  updatePrompt.mockResolvedValue(list);
  typeName('Alpha 2');
  await vi.waitFor(() => expect(onCloseRequested).toHaveBeenCalledOnce());
  closeHandler!({ preventDefault: vi.fn() });
  flushSync();
  dialog()!.querySelector<HTMLButtonElement>('button')!.click();
  await vi.waitFor(() => expect(destroyWindow).toHaveBeenCalledOnce());
  expect(updatePrompt).toHaveBeenCalledWith('a', 'Alpha 2', 'Alpha text');
});

it('keeps the window open when saving before close fails', async () => {
  updatePrompt.mockRejectedValue(new Error('save failed'));
  typeName('Alpha 2');
  await vi.waitFor(() => expect(onCloseRequested).toHaveBeenCalledOnce());
  closeHandler!({ preventDefault: vi.fn() });
  flushSync();
  dialog()!.querySelector<HTMLButtonElement>('button')!.click();
  await vi.waitFor(() => expect(target.textContent).toContain('save failed'));
  expect(dialog()).not.toBeNull();
  expect(input().value).toBe('Alpha 2');
  expect(destroyWindow).not.toHaveBeenCalled();
});

it('keeps pending navigation while a save is already in progress', async () => {
  let finish!: (value: typeof list) => void;
  updatePrompt.mockReturnValue(new Promise((resolve) => { finish = resolve; }));
  typeName('Alpha 2');
  button('Save').click();
  await vi.waitFor(() => expect(updatePrompt).toHaveBeenCalledOnce());
  button('Beta').click();
  flushSync();
  expect(dialog()).not.toBeNull();
  dialog()!.querySelector<HTMLButtonElement>('button')!.click();
  finish({ ...list, prompts: [{ ...list.prompts[0], name: 'Alpha 2' }, list.prompts[1]] });
  await vi.waitFor(() => expect(input().value).toBe('Beta'));
  expect(updatePrompt).toHaveBeenCalledOnce();
});

it('guards leaving Settings while a style is dirty', () => {
  typeName('Alpha 2');
  const leave = vi.fn();
  (app as unknown as { requestLeave: (action: () => void) => void }).requestLeave(leave);
  flushSync();
  expect(dialog()?.textContent).toContain('Discard changes?');
  expect(leave).not.toHaveBeenCalled();
  button('Cancel').click();
  flushSync();
  expect(leave).not.toHaveBeenCalled();

  (app as unknown as { requestLeave: (action: () => void) => void }).requestLeave(leave);
  flushSync();
  button('Discard').click();
  expect(leave).toHaveBeenCalledOnce();
});

it('sends only one create request when Save is clicked twice', async () => {
  let finish!: (value: typeof list) => void;
  createPrompt.mockReturnValue(new Promise((resolve) => { finish = resolve; }));
  button('New style').click();
  flushSync();
  const textarea = target.querySelector('textarea')!;
  textarea.value = 'New text';
  textarea.dispatchEvent(new Event('input'));
  flushSync();
  const save = button('Save');
  save.click();
  save.click();
  expect(createPrompt).toHaveBeenCalledOnce();
  finish({
    ...list,
    createdId: 'created',
    prompts: [...list.prompts, prompt('created', 'Created')],
  });
  await vi.waitFor(() => expect(input().value).toBe('Created'));
  expect(createPrompt).toHaveBeenCalledOnce();
});
