// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';
import type { CoachTarget } from '../coach-context';
import type { CoachDraft } from '../types/CoachDraft';
import type { CoachQuestion } from '../types/CoachQuestion';
import type { CoachResult } from '../types/CoachResult';
import type { PracticeNote } from '../types/PracticeNote';
import type { PromptList } from '../types/PromptList';

const rpc = vi.hoisted(() => ({
  coachOpen: vi.fn(), coachSamples: vi.fn(), coachBuild: vi.fn(), coachAnswer: vi.fn(),
  coachPractice: vi.fn(), coachInterpret: vi.fn(), coachChoose: vi.fn(), coachSave: vi.fn(),
  coachCancel: vi.fn(), onCoachProgress: vi.fn(), onCoachResult: vi.fn(),
}));
vi.mock('../api', () => rpc);
const { default: NoteStyleCoach } = await import('./NoteStyleCoach.svelte');

const original: CoachDraft = {
  id: 'draft', name: 'My note style', isAdjustment: false, acceptedChange: false,
  spec: null, question: null, note: null, proposal: null,
};
const spec: NonNullable<CoachDraft['spec']> = {
  compilerVersion: 1, sentenceMin: 1, sentenceMax: 3, quoteMax: 0, quoteRequired: false,
  voice: 'thirdPerson', pronouns: 'neutral', emphasis: [],
  sections: [{ heading: 'Assessment', required: true, guidance: 'Use relevant details.' }],
};
const question: CoachQuestion = {
  text: 'Should quotes appear?', options: ['No quotes', 'Include one when available'], allowSkip: true,
};
const before: PracticeNote = { text: 'Assessment:\nThe client found a routine.', passed: 8, total: 9, failed: ['length'] };
const after: PracticeNote = { text: 'Assessment:\nThe client took a break.', passed: 9, total: 9, failed: [] };
const active = { ...original, spec, note: before };
const proposed = { ...active, proposal: { id: 'revision', change: 'Use fewer sentences.', note: after } };

let target: HTMLElement;
let app: ReturnType<typeof NoteStyleCoach>;
let resultHandler: (result: CoachResult) => void;
let onSaved: (styles: PromptList) => void;

function button(name: string) {
  return [...target.querySelectorAll('button')].find((b) => b.textContent?.trim() === name)!;
}

function textInput(input: HTMLInputElement | HTMLTextAreaElement, value: string) {
  input.value = value;
  input.dispatchEvent(new Event('input'));
  flushSync();
}

async function start(coachTarget: CoachTarget = {}) {
  onSaved = vi.fn();
  app = mount(NoteStyleCoach, { target, props: {
    target: coachTarget, onClose: vi.fn(), onDraft: () => true, onSaved,
    availability: { reason: null, message: null, sessionMessage: null, retry: null },
  } });
  if (!coachTarget.styleId) await vi.waitFor(() => expect(button('Continue').disabled).toBe(false));
  else await vi.waitFor(() => expect(rpc.coachPractice).toHaveBeenCalledWith('draft'));
}

function emitNote(draft: CoachDraft, error: string | null = null) {
  resultHandler({ jobId: draft.proposal?.id ?? 'practice', draft, error });
  flushSync();
}

beforeEach(() => {
  vi.resetAllMocks();
  target = document.createElement('div');
  document.body.append(target);
  if (!HTMLDialogElement.prototype.showModal) {
    HTMLDialogElement.prototype.showModal = function () { this.open = true; };
  }
  rpc.coachOpen.mockResolvedValue(original);
  rpc.onCoachProgress.mockResolvedValue(() => {});
  rpc.onCoachResult.mockImplementation(async (handler) => { resultHandler = handler; return () => {}; });
  rpc.coachPractice.mockResolvedValue('practice');
  rpc.coachCancel.mockResolvedValue(undefined);
});

afterEach(() => {
  unmount(app);
  target.remove();
});

it('validates intake, asks in order, and only shows formatting results in the conversation', async () => {
  const reply = () => target.querySelector<HTMLInputElement>('input[aria-label="Reply to the coach"]')!;
  let finishReading!: (draft: CoachDraft) => void;
  rpc.coachBuild.mockReturnValue(new Promise<CoachDraft>((resolve) => (finishReading = resolve)));
  rpc.coachAnswer.mockResolvedValue({ kind: 'answer', draft: { ...original, spec }, message: 'I’ll allow up to two exact quotes.' });
  await start();
  button('Continue').click();
  flushSync();
  expect(target.querySelector('[role=alert]')?.textContent).toMatch(/Enter an example/);
  expect(rpc.coachBuild).not.toHaveBeenCalled();
  const example = 'é'.repeat(100);
  textInput(target.querySelector('textarea')!, example);
  button('Continue').click();
  await vi.waitFor(() => expect(rpc.coachBuild).toHaveBeenCalledWith(example, 'draft'));
  expect(target.querySelectorAll('progress')).toHaveLength(1);
  expect(target.textContent).toContain('Reading your example');
  expect(reply().disabled).toBe(true);
  expect(button('Continue').disabled).toBe(true);
  expect(button('Cancel').disabled).toBe(false);
  expect(target.querySelector('textarea')).toBeNull();

  finishReading({ ...original, spec, question });
  await vi.waitFor(() => expect(button('No quotes')).toBeDefined());
  expect(button('Skip, use your best guess')).toBeDefined();
  textInput(reply(), '2 quotes');
  button('Send').click();
  await vi.waitFor(() => expect(rpc.coachPractice).toHaveBeenCalledWith('draft'));
  expect(rpc.coachAnswer).toHaveBeenCalledWith('draft', '2 quotes');
  expect(target.textContent).toContain('I’ll allow up to two exact quotes.');
  expect(target.querySelectorAll('progress')).toHaveLength(1);
  emitNote(active);
  expect(target.querySelector('[role=log]')?.textContent).toContain('8 of 9 formatting checks passed');
  const paper = target.querySelector('[aria-label="Practice note"]')!;
  expect(paper.textContent).toContain('Practice session, not a real client');
  expect(paper.textContent).not.toContain('formatting checks');
  expect(button('Continue').disabled).toBe(false);
});

it('keeps the accepted adjustment until Use This and preserves it after a failed rewrite', async () => {
  const adjusted = { ...active, isAdjustment: true };
  const revision = { ...proposed, isAdjustment: true };
  const accepted = { ...adjusted, acceptedChange: true, note: after };
  rpc.coachOpen.mockResolvedValue({ ...adjusted, note: null });
  rpc.coachInterpret.mockResolvedValue({ kind: 'writing', jobId: 'revision' });
  rpc.coachChoose.mockResolvedValueOnce(adjusted).mockResolvedValueOnce(accepted);
  rpc.coachSave.mockResolvedValue({ prompts: [], activeId: null, createdId: 'existing' });
  await start({ styleId: 'existing' });
  expect(target.querySelector('[aria-current=step]')?.textContent).toBe('Check and adjust');
  emitNote(adjusted);
  expect(button('Continue').disabled).toBe(true);
  button('Make it shorter').click();
  emitNote(revision);
  expect(button('Continue').disabled).toBe(true);
  expect(target.querySelector('del')?.textContent).toContain('found a routine');
  expect(target.querySelector('ins')?.textContent).toContain('took a break');
  expect(target.querySelector('input[aria-label="Reply to the coach"]')).toBeNull();
  button('Before').click();
  flushSync();
  expect(target.querySelector('pre')?.textContent).toBe(before.text);
  button('After').click();
  flushSync();
  expect(target.querySelector('pre')?.textContent).toBe(after.text);
  button('Keep the Old One').click();
  await vi.waitFor(() => expect(target.querySelector('pre')?.textContent).toBe(before.text));
  expect(button('Continue').disabled).toBe(true);
  button('Make it shorter').click();
  emitNote(adjusted, 'I couldn’t write the practice note. Try again.');
  expect(target.querySelector('pre')?.textContent).toBe(before.text);

  button('Make it shorter').click();
  emitNote(revision);
  button('Use This').click();
  await vi.waitFor(() => expect(button('Continue').disabled).toBe(false));
  expect(target.querySelector('pre')?.textContent).toBe(after.text);
  button('Continue').click();
  flushSync();
  expect(target.querySelector<HTMLInputElement>('input[type=checkbox]')?.checked).toBe(true);
  button('Save').click();
  await vi.waitFor(() => expect(onSaved).toHaveBeenCalled());
  expect(rpc.coachSave).toHaveBeenCalledWith('draft', 'My note style', true);
});
