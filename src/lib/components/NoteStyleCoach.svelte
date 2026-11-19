<script lang="ts">
  import { onMount, tick, untrack } from 'svelte';
  import { coachAnswer, coachBuild, coachCancel, coachChoose, coachInterpret, coachOpen, coachPractice, coachSamples, coachSave, onCoachProgress, onCoachResult, retrySummaryModel } from '../api';
  import { exampleError, readExample } from '../coach-example';
  import type { CoachTarget } from '../coach-context';
  import type { Check } from '../types/Check';
  import type { CoachAvailability } from '../types/CoachAvailability';
  import type { CoachDraft } from '../types/CoachDraft';
  import type { CoachQuestion } from '../types/CoachQuestion';
  import type { CoachResult } from '../types/CoachResult';
  import type { CoachSample } from '../types/CoachSample';
  import type { PracticeNote } from '../types/PracticeNote';
  import type { PromptList } from '../types/PromptList';

  let { target, availability, onClose, onSaved, onDraft }: {
    target: CoachTarget;
    availability: CoachAvailability | null;
    onClose: () => void;
    onSaved: (list: PromptList) => void;
    onDraft: (id: string) => boolean;
  } = $props();

  let dialog: HTMLDialogElement;
  let transcript = $state<HTMLDivElement>();
  let step = $state(untrack(() => target.styleId ? 2 : 1));
  let draft = $state<CoachDraft | null>(null);
  let ready = $state(false);
  let example = $state('');
  let samples = $state<CoachSample[] | null>(null);
  let showSamples = $state(untrack(() => !!target.samples));
  let loadingFile = $state(false);
  let name = $state('');
  let useNext = $state(true);
  let error = $state('');
  let work = $state<'opening' | 'reading' | 'writing' | 'reply' | 'choosing' | 'saving' | null>('opening');
  let attempt = $state(0);
  let messages = $state<{ side: 'coach' | 'person'; text: string }[]>([]);
  let reply = $state('');
  let clarify = $state<CoachQuestion | null>(null);
  let options = $state<string[]>([]);
  let contrast = $state<'Changes' | 'Before' | 'After'>('Changes');
  let confirmStop = $state(false);
  let lastFocus: HTMLElement | null = null;
  let disposed = false;
  let committed = false;

  const suggestions = [
    'Make it shorter', 'One sentence per section', 'Include an exact quote when available',
    'Add a section', 'Remove a section', 'Make a section optional',
  ];
  const checkNames: Record<Check, string> = {
    sectionsPresent: 'required sections', noRepeats: 'repeated headings',
    optionalOmission: 'optional sections', length: 'section length',
    exactQuotes: 'exact quotes', quoteAttribution: 'who said each quote',
    noNames: 'personal names', pronouns: 'pronouns', noPreamble: 'opening text',
  };

  let blocked = $derived(!availability || availability.reason !== null);
  let question = $derived(draft?.question ?? clarify);
  let note = $derived(draft?.note ?? null);
  let proposal = $derived(draft?.proposal ?? null);
  let canContinue = $derived(note && !work && !blocked && !proposal && (!draft?.isAdjustment || draft.acceptedChange));
  let pasteArea = $state<HTMLTextAreaElement>();
  let stepTitle = $state<HTMLHeadingElement>();
  let keepGoing = $state<HTMLButtonElement>();

  // The event can arrive before the launch request resolves, especially with a fake engine.
  onMount(() => {
    dialog.showModal();
    const progress = onCoachProgress((event) => {
      if (event.draftId !== draft?.id || disposed) return;
      attempt = event.attempt;
    });
    const result = onCoachResult(finishPractice);
    void Promise.all([progress, result]).then(open).catch(() => {
      if (!disposed) {
        work = null;
        error = 'I couldn’t open the coach. Close it and try again.';
      }
    });

    return () => {
      disposed = true;
      void progress.then((remove) => remove()).catch(() => {});
      void result.then((remove) => remove()).catch(() => {});
      if (draft && !committed) void coachCancel(draft.id).catch(() => {});
    };
  });

  async function open() {
    const opened = await coachOpen(target.styleId ?? null);
    if (disposed || !onDraft(opened.id)) {
      void coachCancel(opened.id).catch(() => {});
      return;
    }
    draft = opened;
    name = opened.name;
    ready = true;
    work = null;
    if (opened.isAdjustment) {
      say('Here’s the style you saved. I’ll write a practice note so you can try a change.');
      await practice();
    } else {
      if (showSamples) void loadSamples();
      await tick();
      if (!confirmStop) pasteArea?.focus();
    }
  }

  function say(text: string, side: 'coach' | 'person' = 'coach') {
    messages.push({ text, side });
  }

  $effect(() => {
    if (messages.length || work) {
      void tick().then(() => { if (transcript) transcript.scrollTop = transcript.scrollHeight; });
    }
  });

  async function loadSamples() {
    showSamples = true;
    if (samples) return;
    try { samples = await coachSamples(); }
    catch { if (!disposed) error = 'I couldn’t open the sample notes. Try again.'; }
  }

  async function loadFile(input: HTMLInputElement) {
    const file = input.files?.[0];
    input.value = '';
    if (!file) return;
    loadingFile = true;
    error = '';
    try {
      const text = await readExample(file);
      if (!disposed) example = text;
    } catch (problem) {
      if (!disposed) error = problem instanceof Error ? problem.message : 'I couldn’t read that file.';
    } finally {
      loadingFile = false;
    }
  }

  function describe(current: CoachDraft) {
    const spec = current.spec;
    if (!spec) return '';
    return `I found ${spec.sections.map((section) => section.heading).join(', ')}. I’ll use ${spec.sentenceMin}–${spec.sentenceMax} sentences per section and ${spec.voice === 'clinicianFirstPerson' ? 'write from the clinician’s perspective' : 'write in third person'}.`;
  }

  function ask(current: CoachQuestion | null | undefined) {
    if (current) say(current.text);
  }

  async function build() {
    if (!draft || work || blocked) return;
    error = exampleError(example) ?? '';
    if (error) return;
    work = 'reading';
    step = 2;
    await tick();
    if (!confirmStop) stepTitle?.focus();
    if (disposed) return;
    const pending = coachBuild(example, draft.id);
    try {
      const built = await pending;
      if (disposed) return;
      draft = built;
      example = '';
      say(describe(built));
      ask(built.question);
      work = null;
      if (!built.question) await practice();
    } catch (problem) {
      if (disposed) return;
      work = null;
      step = 1;
      error = String(problem);
    }
  }

  async function practice() {
    if (!draft) return;
    work = 'writing';
    error = '';
    attempt = 0;
    try { await coachPractice(draft.id); }
    catch (problem) {
      if (disposed) return;
      error = String(problem);
      work = null;
    }
  }

  function reportFormatting(result: PracticeNote) {
    say(`Matches your style: ${result.passed} of ${result.total} formatting checks passed.${result.failed.length ? ` Needs another look: ${result.failed.map((check) => checkNames[check]).join(', ')}.` : ''}`);
  }

  function finishPractice(result: CoachResult) {
    if (disposed || draft?.id !== result.draft.id) return;
    draft = result.draft;
    work = null;
    if (result.error) {
      error = result.error;
      return;
    }
    if (draft.proposal) {
      contrast = 'Changes';
      say(draft.proposal.change);
      reportFormatting(draft.proposal.note);
      say('Which version would you like to keep?');
    } else if (draft.note) {
      reportFormatting(draft.note);
      say('Anything you’d like different?');
    }
  }

  async function respond(answer: string | null) {
    if (!draft || blocked || work || proposal) return;
    const exampleQuestion = draft.question;
    const message = answer?.trim() ?? null;
    if (message === '' || (message === null && !exampleQuestion)) return;
    work = 'reply';
    attempt = 0;
    error = '';
    clarify = null;
    options = [];
    reply = '';
    try {
      if (exampleQuestion) {
        const received = await coachAnswer(draft.id, message);
        if (disposed || received.kind !== 'answer') return;
        say(message ?? 'Skip, use your best guess', 'person');
        draft = received.draft;
        say(received.message);
        ask(draft.question);
        work = null;
        if (!draft.question) await practice();
        return;
      }
      say(message!, 'person');
      // Mark the job as writing before calling Rust so that a fast result wins.
      work = 'writing';
      const received = await coachInterpret(draft.id, message!);
      if (disposed) return;
      if (received.kind === 'writing') return;
      work = null;
      if (received.kind === 'question') {
        clarify = received.question;
        ask(clarify);
      } else if (received.kind === 'unsupported') {
        options = received.suggestions;
        say('I can change length, quotes and sections. Try one of these.');
      }
    } catch (problem) {
      if (disposed) return;
      work = null;
      error = String(problem);
      reply = message ?? '';
    }
  }

  async function choose(useThis: boolean) {
    if (!draft || !proposal || blocked || work) return;
    work = 'choosing';
    error = '';
    try {
      const chosen = await coachChoose(draft.id, proposal.id, useThis);
      if (disposed) return;
      draft = chosen;
      say(useThis ? 'Use This' : 'Keep the Old One', 'person');
      say(useThis ? 'Done. I’ll use that change. Anything else?' : 'I’ll keep the earlier version. Anything else?');
    } catch (problem) { if (!disposed) error = String(problem); }
    finally { if (!disposed) work = null; }
  }

  async function continueToSave() {
    if (!canContinue) return;
    step = 3;
    error = '';
    await tick();
    stepTitle?.focus();
  }

  async function save() {
    if (!draft || blocked || work || !name.trim()) return;
    work = 'saving';
    error = '';
    try {
      const saved = await coachSave(draft.id, name, useNext);
      if (disposed) return;
      committed = true;
      onSaved(saved);
    } catch (problem) {
      if (disposed) return;
      error = String(problem);
      work = null;
    }
  }

  async function toggleCancel() {
    if (work === 'saving') return;
    if (confirmStop) {
      confirmStop = false;
      await tick();
      if (lastFocus?.isConnected) lastFocus.focus();
      else stepTitle?.focus();
    } else {
      lastFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
      confirmStop = true;
      await tick();
      keepGoing?.focus();
    }
  }

  function selectComparison(label: string) {
    if (label === 'Changes' || label === 'Before' || label === 'After') contrast = label;
  }

  let beforeLines = $derived(note?.text.split('\n') ?? []);
  let afterLines = $derived(proposal?.note.text.split('\n') ?? []);
</script>

<dialog bind:this={dialog} aria-label="Note Style Coach" oncancel={(event) => { event.preventDefault(); void toggleCancel(); }}>
  <div class="sheet">
    {#if confirmStop}
      <div class="discard">
        <h2>Stop making this style?</h2>
        <p>Your answers and changes won’t be kept.</p>
        <footer>
          <button onclick={onClose}>Stop</button>
          <button class="primary" bind:this={keepGoing} onclick={toggleCancel}>Keep Going</button>
        </footer>
      </div>
    {:else}
      <ol class="steps" aria-label="Steps">
        {#each ['Your example note', 'Check and adjust', 'Save'] as label, i (label)}
          <li aria-current={step === i + 1 ? 'step' : undefined} class:complete={step > i + 1}>{label}</li>
        {/each}
      </ol>
      {#if step === 1}
        <div class="intake">
          <h2 bind:this={stepTitle} tabindex="-1">Your example note</h2>
          <p>Paste a note you’d like your future notes to follow. Your example stays on this Mac.</p>
          <textarea bind:this={pasteArea} bind:value={example} aria-label="Example note" disabled={!ready || loadingFile} placeholder="Paste an example note…"></textarea>
          <div class="input-actions">
            <label class="file-action">Open Text File…
              <input type="file" aria-label="Open text file" accept=".txt,.md" disabled={!ready || loadingFile} onchange={(event) => void loadFile(event.currentTarget)} />
            </label>
            <button disabled={!ready || loadingFile} onclick={() => showSamples ? (showSamples = false) : void loadSamples()}>Use a Sample Note</button>
          </div>
          {#if showSamples}
            <div class="input-actions" aria-label="Fictional sample notes">
              {#each samples ?? [] as sample (sample.name)}
                <button disabled={loadingFile} onclick={() => { example = sample.text; error = ''; showSamples = false; pasteArea?.focus(); }}>{sample.name}</button>
              {/each}
            </div>
          {/if}
        </div>
      {:else if step === 2}
        <h2 class="sr-only" bind:this={stepTitle} tabindex="-1">Check and adjust</h2>
        <div class="preview">
          <section class="conversation" aria-label="Coach conversation">
            <div class="messages" role="log" aria-live="polite" aria-label="Coach messages" bind:this={transcript}>
              {#each messages as message}
                <p class:person={message.side === 'person'}>{message.text}</p>
              {/each}
              {#if work === 'opening' || work === 'reading' || work === 'writing'}
                <div class="progress" role="status">
                  <span>{work === 'reading' ? 'Reading your example…' : work === 'opening' ? 'Opening your style…' : 'Writing your practice note…'}</span>
                  <progress aria-label={work === 'reading' ? 'Reading your example' : 'Preparing your practice note'} max="3" value={attempt || undefined}></progress>
                </div>
              {/if}
            </div>
            {#if proposal}
              <div class="answers">
                <button disabled={blocked || !!work} onclick={() => void choose(false)}>Keep the Old One</button>
                <button class="primary" disabled={blocked || !!work} onclick={() => void choose(true)}>Use This</button>
              </div>
            {:else}
              {#if !work && (question || note)}
                <div class="answers">
                  {#each (question?.options ?? (options.length ? options : suggestions)) as option (option)}
                    <button disabled={blocked} onclick={() => void respond(option)}>{option}</button>
                  {/each}
                  {#if question?.allowSkip}<button disabled={blocked} onclick={() => void respond(null)}>Skip, use your best guess</button>{/if}
                </div>
              {/if}
              {#if draft?.spec && !question && !note && !work}
                <button disabled={blocked} onclick={practice}>Try writing the practice note again</button>
              {/if}
              <form class="reply" onsubmit={(event) => { event.preventDefault(); void respond(reply); }}>
                <input bind:value={reply} aria-label="Reply to the coach" maxlength="500"
                  placeholder={work === 'reading' ? 'The coach is reading your example…' : question ? 'Or type your own answer…' : note ? 'What would you like different?' : 'You can ask for changes once the note is ready'}
                  disabled={blocked || !!work || (!question && !note)} />
                <button type="submit" disabled={blocked || !!work || (!question && !note) || !reply.trim()}>Send</button>
              </form>
            {/if}
          </section>
          <section class="practice-note" aria-label="Practice note">
            <div class="comparison" role="group" aria-label="Compare the notes">
              {#each ['Changes', 'Before', 'After'] as label}
                <button disabled={!proposal || !!work} aria-pressed={!!proposal && contrast === label}
                  onclick={() => selectComparison(label)}>{label}</button>
              {/each}
            </div>
            <div class="paper">
              {#if proposal && contrast === 'Changes'}
                <div class="diff">
                  {#each beforeLines.filter((line) => line.trim() && !afterLines.includes(line)) as line}<del>{line}</del>{/each}
                  {#each afterLines as line}
                    {#if line.trim() && !beforeLines.includes(line)}<ins>{line}</ins>{:else}<span>{line || ' '}</span>{/if}
                  {/each}
                </div>
              {:else if note}
                <pre>{proposal && contrast === 'After' ? proposal.note.text : note.text}</pre>
              {:else}
                <p>Your practice note will appear here.</p>
              {/if}
            </div>
            <div class="practice-status">Practice session, not a real client{#if proposal && contrast === 'Changes'} · <del>Removed</del> · <ins>Added</ins>{/if}</div>
          </section>
        </div>
      {:else}
        <div class="save">
          <h2 bind:this={stepTitle} tabindex="-1">Save your style</h2>
          <p>Give this style a name you’ll recognize when writing your next note.</p>
          <label>Name <input bind:value={name} maxlength="100" disabled={!!work} /></label>
          <label><input type="checkbox" bind:checked={useNext} disabled={!!work} /> Use it for my next sessions</label>
          <small>You can change this any time from the style menu.</small>
          {#if draft}<p>{describe(draft)}</p>{/if}
          <p class="adjust-hint">To make changes later, choose Adjust with the Coach… in Note Styles.</p>
        </div>
      {/if}
      {#if error}<p class="error" role="alert">{error}</p>{/if}
      {#if availability?.message}
        <div class="availability" role="status">
          {availability.message}
          {#if availability.retry === 'tryAgain'}<button onclick={() => void retrySummaryModel().catch(() => (error = 'Couldn’t try again.'))}>Try Again</button>{/if}
        </div>
      {/if}
      <footer>
        <button onclick={toggleCancel} disabled={work === 'saving'}>Cancel</button>
        {#if step === 1}
          <button class="primary" onclick={build} disabled={!ready || blocked || loadingFile || !!work}>Continue</button>
        {:else if step === 2}
          <button class="primary" onclick={continueToSave} disabled={!canContinue}>Continue</button>
        {:else}
          <button class="primary" onclick={save} disabled={blocked || !!work || !name.trim()}>Save</button>
        {/if}
      </footer>
    {/if}
  </div>
</dialog>

<style>
  dialog { box-sizing: border-box; width: min(760px, calc(100vw - 32px)); height: min(640px, calc(100vh - 44px)); padding: 0; border: 1px solid color-mix(in srgb, CanvasText 22%, Canvas); border-radius: 12px; background: Canvas; color: CanvasText; box-shadow: 0 12px 44px #0005; }
  dialog::backdrop { background: #14192365; }
  .sheet { height: 100%; display: flex; flex-direction: column; }
  .steps { display: flex; justify-content: space-around; gap: 8px; margin: 0; padding: 13px 16px; list-style: none; border-bottom: 1px solid color-mix(in srgb, CanvasText 15%, Canvas); font-size: 0.9rem; }
  .steps li { color: color-mix(in srgb, CanvasText 70%, Canvas); }
  .steps [aria-current] { color: CanvasText; font-weight: 650; }
  .steps .complete::before { content: '✓ '; }
  h2 { margin: 0 0 8px; }
  .intake, .save, .discard { flex: 1; min-height: 0; overflow: auto; padding: 20px; }
  .intake, .save { display: flex; flex-direction: column; gap: 9px; }
  .intake textarea { min-height: 80px; flex: 1; resize: vertical; font: inherit; padding: 8px; }
  .intake p, .save p { margin: 0; }
  .input-actions { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .file-action { display: inline-flex; align-items: center; gap: 6px; }
  .file-action input { width: 10rem; }
  .preview { flex: 1; min-height: 0; display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); }
  .conversation, .practice-note { min-height: 0; display: flex; flex-direction: column; padding: 12px; gap: 8px; }
  .practice-note { border-left: 1px solid color-mix(in srgb, CanvasText 15%, Canvas); }
  .paper, .messages { min-height: 0; flex: 1; overflow: auto; }
  .messages p, .progress { margin: 4px 0 8px; border-radius: 12px; padding: 7px 10px; width: fit-content; max-width: 86%; overflow-wrap: anywhere; background: color-mix(in srgb, CanvasText 8%, Canvas); }
  .messages .person { margin-left: auto; color: HighlightText; background: Highlight; }
  .progress { display: grid; gap: 4px; }
  .progress progress { width: 100%; }
  .answers, .reply, .comparison { display: flex; gap: 5px; }
  .answers { flex-wrap: wrap; justify-content: flex-end; max-height: 30%; overflow: auto; }
  .reply input { min-width: 0; flex: 1; }
  .comparison { justify-content: center; }
  .comparison [aria-pressed='true'] { font-weight: bold; }
  .paper { padding: 4px 8px; background: color-mix(in srgb, CanvasText 5%, Canvas); border-radius: 6px; }
  .paper pre, .diff { white-space: pre-wrap; overflow-wrap: anywhere; font: inherit; }
  .diff > * { display: block; }
  del { background: #b13d3d26; }
  ins { background: #319b562e; }
  .practice-status { min-height: 2.5em; font-size: 0.8rem; }
  .save { box-sizing: border-box; margin-inline: auto; width: min(100%, 410px); }
  .save label:first-of-type { display: grid; gap: 4px; }
  .save small { margin-inline-start: 1.5em; }
  .save .adjust-hint { margin-top: auto; }
  .discard { align-content: center; margin-inline: auto; }
  footer { display: flex; justify-content: flex-end; gap: 8px; padding: 10px 14px; border-top: 1px solid color-mix(in srgb, CanvasText 15%, Canvas); }
  .discard footer { padding-inline: 0; margin-top: 16px; }
  .primary { color: HighlightText; background: Highlight; border: 1px solid transparent; border-radius: 5px; padding: 3px 9px; }
  .primary:disabled { opacity: 0.5; }
  .error, .availability { margin: 4px 14px; font-size: 0.9rem; }
  .sr-only { position: absolute; width: 1px; height: 1px; overflow: hidden; clip-path: inset(50%); }
</style>
