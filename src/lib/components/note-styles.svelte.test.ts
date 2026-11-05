// @vitest-environment jsdom
// Parity spec 03 AS10: the discard dialog guards a dirty editor.
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';

const prompt = (id: string, name: string, isDefault = false) =>
  ({ id, name, description: '', text: `${name} text`, isDefault, starred: true });
const list = { prompts: [prompt('a', 'Alpha'), prompt('b', 'Beta')], activeId: 'a', createdId: null };

const getPromptList = vi.fn();
const updatePrompt = vi.fn();
vi.mock('../api', () => ({
  getPromptList, updatePrompt, createPrompt: vi.fn(), deletePrompt: vi.fn(), duplicatePrompt: vi.fn(),
  setActivePrompt: vi.fn(), setPromptStarred: vi.fn(),
}));
const { default: NoteStyles } = await import('./NoteStyles.svelte');

let target: HTMLElement;
let app: ReturnType<typeof mount>;

const button = (label: string) =>
  [...target.querySelectorAll('button')].find((b) => b.textContent?.trim() === label)!;
const input = () => target.querySelector<HTMLInputElement>('li input')!;
const dialog = () => target.querySelector('[role="dialog"]');

beforeEach(async () => {
  getPromptList.mockResolvedValue(list);
  target = document.createElement('div');
  app = mount(NoteStyles, { target });
  await vi.waitFor(() => expect(button('Alpha')).toBeTruthy());
  button('Alpha').click();
  flushSync();
});

afterEach(() => { unmount(app); vi.clearAllMocks(); });

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
