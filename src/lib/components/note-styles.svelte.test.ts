// @vitest-environment jsdom
// Parity spec 03 AS10: the discard dialog guards a dirty editor.
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';
import { COACH_CONTEXT, type CoachContext } from '../coach-context';
import type { CoachAvailability } from '../types/CoachAvailability';
import type { StyleSpec } from '../types/StyleSpec';
import { fromStore, writable } from 'svelte/store';

const prompt = (id: string, name: string, isDefault = false) =>
  ({ id, name, description: '', text: `${name} text`, isDefault, showInStyleMenu: true, styleSpec: null as StyleSpec | null });
const list: { prompts: ReturnType<typeof prompt>[]; activeId: string; createdId: string | null } = {
  prompts: [prompt('a', 'Alpha'), prompt('b', 'Beta')], activeId: 'a', createdId: null,
};

const getPromptList = vi.fn();
const updatePrompt = vi.fn();
const createPrompt = vi.fn();
const deletePrompt = vi.fn();
const ready: CoachAvailability = { reason: null, retry: null, message: null, sessionMessage: null };
const availability = writable(ready);
const openCoach = vi.fn();
const availabilityState = fromStore(availability);
const coach: CoachContext = {
  availability: () => availabilityState.current,
  open: openCoach,
  styles: () => null,
  updateStyles: vi.fn(),
  firstNoteTip: () => false,
  dismissTip: vi.fn(),
};

vi.mock('../api', () => ({
  getPromptList, updatePrompt, createPrompt, deletePrompt, duplicatePrompt: vi.fn(),
  setPromptVisibility: vi.fn(),
  openSettings: vi.fn(), openExternalLink: vi.fn(), retrySummaryModel: vi.fn(),
}));
const { default: NoteStyles } = await import('./NoteStyles.svelte');
const { accessState } = await import('../license');

let target: HTMLElement;
let app: ReturnType<typeof NoteStyles>;

const button = (label: string) =>
  [...target.querySelectorAll('button')].find((b) => b.textContent?.trim() === label || ['Edit', 'View'].some((action) => b.getAttribute('aria-label') === `${action} ${label}`))!;
const input = () => target.querySelector<HTMLInputElement>('li input')!;
const dialog = () => target.querySelector('[role="dialog"]');

beforeEach(async () => {
  accessState.set(null);
  availability.set(ready);
  getPromptList.mockResolvedValue(list);
  target = document.createElement('div');
  document.body.append(target);
  app = mount(NoteStyles, { target, context: new Map([[COACH_CONTEXT, coach]]) });
  await vi.waitFor(() => expect(button('Alpha')).toBeTruthy());
  button('Alpha').click();
  flushSync();
});

afterEach(() => { unmount(app); target.remove(); vi.clearAllMocks(); });

it('keeps saved styles readable and deletable while disabling style changes after licence lapse', async () => {
  accessState.set({ kind: 'licenseLapsed' });
  flushSync();
  expect(target.textContent).toContain('Your existing notes are still available.');
  expect(button('New style').disabled).toBe(true);
  expect(target.querySelector<HTMLButtonElement>('[role=switch]')?.disabled).toBe(true);
  expect(button('Make default')).toBeUndefined();
  expect(input().readOnly).toBe(true);
  expect(button('Delete').disabled).toBe(false);
});

it('shares coach availability and only offers retry for recoverable failures', () => {
  availability.set({ reason: 'unsupported', retry: null, message: "Summaries aren't available on this Mac.", sessionMessage: null });
  flushSync();
  expect(button('New Style from Example Note…').getAttribute('aria-disabled')).toBe('true');
  expect(target.textContent).toContain("Summaries aren't available on this Mac.");
  expect(button('New Style from Example Note…').parentElement?.title).toBe("Summaries aren't available on this Mac.");
  expect(button('Try Again')).toBeUndefined();
  availability.set({ reason: 'loadFailed', retry: 'tryAgain', message: "Summaries couldn't load.", sessionMessage: null });
  flushSync();
  expect(button('Try Again')).toBeDefined();
  expect(button('Alpha')).toBeDefined();
  availability.set(ready);
  flushSync();
  button('New Style from Example Note…').click();
  expect(openCoach).toHaveBeenCalled();
});

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

it('shows visibility in the new style draft and keeps one control for saved styles', () => {
  expect(target.querySelectorAll('[role=switch][aria-label="Show in style menu"]')).toHaveLength(0);
  button('New style').click();
  flushSync();
  expect(target.querySelectorAll('[role=switch][aria-label="Show in style menu"]')).toHaveLength(1);
  expect(target.querySelector('[aria-label="Edit Alpha"]')).toBeTruthy();
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
  expect(updatePrompt).toHaveBeenCalledWith('a', 'Alpha 2', 'Alpha text', true, false);
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

it('moves focus to a remaining style action after deleting the style that opened the dialog', async () => {
  deletePrompt.mockResolvedValue({ ...list, prompts: [list.prompts[1]], activeId: 'b' });
  button('Delete').click();
  flushSync();
  button('Delete').click();
  await vi.waitFor(() => expect(button('New Style from Example Note…')).toBe(document.activeElement));
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

it('restores focus before leaving Settings unmounts the editor', async () => {
  typeName('Alpha 2');
  const opener = document.createElement('button');
  document.body.append(opener);
  (app as unknown as { requestLeave: (action: () => void, opener: HTMLElement) => void })
    .requestLeave(() => {
      unmount(app);
      app = mount(NoteStyles, { target, context: new Map([[COACH_CONTEXT, coach]]) });
    }, opener);
  flushSync();
  button('Discard').click();
  await vi.waitFor(() => expect(document.activeElement).toBe(opener));
  opener.remove();
});

it('asks before closing the window with a dirty style', () => {
  const hideWindow = vi.fn();
  typeName('Alpha 2');
  app.requestLeave(hideWindow);
  flushSync();
  expect(dialog()?.textContent).toContain('Discard changes?');
  button('Cancel').click();
  flushSync();
  expect(hideWindow).not.toHaveBeenCalled();
  app.requestLeave(hideWindow);
  flushSync();
  button('Discard').click();
  expect(hideWindow).toHaveBeenCalledOnce();
  flushSync();
  expect(input().value).toBe('Alpha');
});

it('saves a dirty style before closing the window', async () => {
  const hideWindow = vi.fn();
  updatePrompt.mockResolvedValue(list);
  typeName('Alpha 2');
  app.requestLeave(hideWindow);
  flushSync();
  dialog()!.querySelector<HTMLButtonElement>('button')!.click();
  await vi.waitFor(() => expect(hideWindow).toHaveBeenCalledOnce());
  expect(updatePrompt).toHaveBeenCalledWith('a', 'Alpha 2', 'Alpha text', true, false);
});

it('keeps the window open when saving before close fails', async () => {
  const hideWindow = vi.fn();
  updatePrompt.mockRejectedValue(new Error('save failed'));
  typeName('Alpha 2');
  app.requestLeave(hideWindow);
  flushSync();
  dialog()!.querySelector<HTMLButtonElement>('button')!.click();
  await vi.waitFor(() => expect(target.textContent).toContain('save failed'));
  expect(dialog()).not.toBeNull();
  expect(input().value).toBe('Alpha 2');
  expect(hideWindow).not.toHaveBeenCalled();
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
  await vi.waitFor(() => expect(dialog()?.contains(document.activeElement)).toBe(true));
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

it('keeps coach origin for a name change and confirms before detaching changed text', async () => {
  unmount(app);
  const styleSpec: StyleSpec = {
    sections: [{ heading: 'Assessment', required: true, guidance: 'Use relevant details.' }],
    sentenceMin: 1, sentenceMax: 2, quoteMax: 0, quoteRequired: false,
    pronouns: 'neutral', voice: 'thirdPerson', emphasis: [], compilerVersion: 1,
  };
  const made = { ...list.prompts[0], styleSpec };
  const withCoach = { ...list, prompts: [made, list.prompts[1]] };
  getPromptList.mockResolvedValue(withCoach);
  updatePrompt.mockResolvedValueOnce({ ...withCoach, prompts: [{ ...made, name: 'Renamed' }, list.prompts[1]] });
  app = mount(NoteStyles, { target, context: new Map([[COACH_CONTEXT, coach]]) });
  await vi.waitFor(() => expect(button('Adjust with the Coach…')).toBeDefined());
  button('Alpha').click();
  flushSync();
  typeName('Renamed');
  button('Save').click();
  await vi.waitFor(() => expect(updatePrompt).toHaveBeenCalledWith('a', 'Renamed', 'Alpha text', true, false));
  await vi.waitFor(() => expect(button('Adjust with the Coach…')).toBeDefined());

  const text = target.querySelector('textarea')!;
  text.value = 'Hand edited';
  text.dispatchEvent(new Event('input'));
  flushSync();
  button('Save').click();
  flushSync();
  expect(dialog()?.textContent).toContain('Save as a hand-edited style?');
  button('Cancel').click();
  await vi.waitFor(() => expect(button('Save').disabled).toBe(false));
  expect(updatePrompt).toHaveBeenCalledTimes(1);

  updatePrompt.mockResolvedValueOnce({ ...list, prompts: [{ ...made, name: 'Renamed', text: 'Hand edited', styleSpec: null }, list.prompts[1]] });
  button('Save').click();
  flushSync();
  button('Save text change').click();
  await vi.waitFor(() => expect(updatePrompt).toHaveBeenLastCalledWith('a', 'Renamed', 'Hand edited', true, true));
  await vi.waitFor(() => expect(button('Adjust with the Coach…')).toBeUndefined());
});
