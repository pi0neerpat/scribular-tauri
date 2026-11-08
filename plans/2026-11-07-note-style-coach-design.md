# Note Style Coach for scribular-tauri: design

Date: 2026-11-07. Status: draft for Patrick's approval.
Source feature: https://github.com/pi0neerpat/scribular-note-style-coach (hackathon demo, Python + Ollama).
Marks used below: **[Patrick]** a decision Patrick made; **[call]** a call made while drafting, open to change; **[unverified]** not yet tested.

## 1. Intent and why

A therapist can describe how their notes should read, but turning that into a reliable summary prompt today takes founder help, email, model testing and manual installation, and it repeats on every change. The note-style coach lets a therapist do this alone, inside Scribular: paste a note they like, see Scribular write a practice note in that style, and adjust it by typing plain requests.

The feature ships as two parts:

1. **Part 1, model swap (prerequisite).** Gemma 4 E2B becomes the only summary model. The coach depends on it.
2. **Part 2, the coach.** A three-step flow in Note Styles.

## 2. Goals, non-goals, anti-goals

**Goals**
- A non-technical therapist goes from "a note I like" to a saved, working style without help and without seeing words like "prompt" or "preferences". **[Patrick]** "Simplest, least complicated, most hand-holding."
- Everything runs on the device: example note, practice notes, chat, inference. No cloud fallback.
- Styles made by the coach are ordinary entries in Note Styles. Summarization, existing styles and existing sessions keep working.

**Non-goals (this version)**
- Recording, transcription, diarization, EHR, marketplace, cloud coaching, model-weight training.
- Improving a style from feedback on a real session's note (the demo's "Improve template"). **[call]** Not in Patrick's list.
- Style version history and rollback. **[Patrick]** Not in the must-have list. Compare-and-undo inside the flow (section 7.4) is the only undo.
- PDF/Word input. Pasted text, `.txt` and `.md` only.
- Any other summary model, including a fallback model. **[Patrick]**

**Anti-goals (failure even if it technically works)**
- A label, score or wording that implies the note is correct or "confident". The checks measure formatting only.
- Any therapist-visible jargon.
- Existing styles, existing summaries or the summary queue regressing.
- Example-note facts leaking into unrelated sessions.

## 3. Constraints

- App: Tauri (Rust + Svelte), SQLite store, embedded llama.cpp through `llama-cpp-2 =0.1.158`. One model is loaded at a time through the `Summarizer` queue. The same queue serves real summaries, so practice work must not starve them.
- A style is today `Prompt { id, name, description, text, isDefault, starred }`, max 8,000 characters (`store/prompts.rs`). Summaries wrap it with `chunk_prompt` / `combine_prompt` (`llm/prompt.rs`).
- Minimum supported Mac: **8 GB RAM**. **[Patrick]**
- Coding follows the existing workflow: implementation by Codex CLI on the VM, independent review by Claude CLI. Tests follow `CLAUDE.md` (assert transcript phrases, never summary wording).

## 4. Approaches considered

Rollout, presented to Patrick: (A) Gemma 4 only for the coach, 2B kept as the default elsewhere; (B) coach on 2B in a reduced form; (C) coach on 2B with a warning. **Patrick rejected all three:** Gemma 4 is the only model. That removes the fallback question.

Porting the coach: running the Python coach as a sidecar needs Python and Ollama installed, which the self-contained app cannot ship. **Port to Rust**, using the existing queue. Only one sensible way.

Flow layout, shown as mockups: (A) step-by-step, one screen per step; (B) workbench with chat beside the live note. **Patrick chose A** because the users need the simplest, most guided path.

Model variant: Gemma 4 E4B (about 6.6 GB, tested on a 16 GB Mac) or E2B (tested on an 8 GB Mac VM). With 8 GB as the minimum, **E2B** is chosen. **[Patrick]**

## 5. Correction to an earlier assumption

The demo's example-note reading (`extract_preferences`) and prompt compiler (`compile_prompt`) are plain deterministic code (regex and templates), not model calls. The model is used for only two things: writing the practice note, and interpreting free-text chat requests. The chat interpreter already has deterministic fallbacks and strict validation (typed patches, allowed paths only, bounded values). This makes the port lighter and less dependent on the small model than first assumed.

## 6. Part 1: model swap (prerequisite)

**6.1 No separate feasibility test. [Patrick]** The hackathon already demonstrated Gemma 4 (E4B and E2B) on real transcripts, so the plan does not start with a go/no-go spike. One distinction stays in the plan as ordinary verification, not a gate: the hackathon ran the model through Ollama, while the app loads it through its pinned `llama-cpp-2 0.1.158` (the source contains `LLM_ARCH_GEMMA4`). The first real-model run in section 9 therefore also covers load, chat template with thinking off, memory beside the speech model, and speed on an 8 GB Mac. If it fails, the builder reports it as a blocker (options: bump `llama-cpp-2`, which has 0.1.160 in the local registry, or return to Patrick).

**6.2 Changes**
- `models/manifest.rs`: add a pinned Gemma 4 E2B entry (exact upstream commit, size, SHA-256, license URL). The GGUF source, size and license terms are **[unverified]**; the builder pins and checks them. Make it `DEFAULT_LLM`; remove `GEMMA_2_2B`.
- `llm/llama.rs`: a `Params` arm for Gemma 4 (context 8192, matching the coach's verified setting; low temperature, e.g. 0.2 with top_p 0.9, as the coach used; thinking disabled). Final values are the builder's, justified by the section 9 checks.
- Settings migration: a stored `SUMMARY_MODEL_ID` of `gemma-2-2b-it-q4km` maps to the new default.
- Download: on update, the new model downloads through the existing flow. Until it is ready, the existing "model isn't downloaded yet" state applies. `FirstRun` and any size text change from about 1.7 GB to the real size.
- Remove the summary-model switcher UI (one model left). **[call]** Keep the backend selection plumbing and its tests; remove only dead UI.
- Hardware gate **[Patrick]**: it runs before any download starts, so a Mac that cannot run Gemma 4 never downloads it. On such a Mac the app still records and transcribes. Summaries are not possible, and the app says so plainly and in the places a therapist would look for them: the summary area of a session, Note Styles (the coach entry is disabled), Settings, and first run. Suggested wording: "This Mac can't write summaries because of its hardware. Recording and transcription still work." Existing summaries stay viewable. The same state applies if the model fails to load on a Mac that passed the gate. The RAM threshold comes from section 9 measurements. **[unverified]**
- 2B is removed for everyone, including Macs that fail the gate. **[Patrick]** On a capable Mac the 2B file is deleted only after Gemma 4 has downloaded, passed its SHA-256 check and written a short test summary on that Mac, so a failed update never leaves a Mac with no model. **[call]**
- Existing sessions keep their recorded model id. Regenerate uses the new model.
- Re-run the built-in styles (copied from the Electron app and tuned on the older model) against Gemma 4 E2B on the bundled fixtures, and report any that now produce clearly worse output. Fixing them is out of scope.

## 7. Part 2: the coach

### 7.1 Entry
Settings → Note Styles → "+ New from example note". **[call]** Also, for a coach-made style, an "Adjust with the coach" button that reopens Steps 2-3 on the saved style (cheap, since the structured spec is stored).

### 7.2 Step 1, your example note
Paste, upload `.txt`/`.md`, or pick one of four bundled fictional sample notes (narrative, DAP, SOAP, brief CBT). One button: "Build my style". Plain-language errors: empty, too short, too long (exact limits chosen by the builder within the 8,000-character prompt cap).

The example note's content is read only for structure (headings, sentence counts, quote counts, pronouns, voice). Its facts are not kept or used.

### 7.3 Step 2, see it work
- The app writes a practice note from a bundled fictional session and labels it "Practice session, not a real client".
- Above the note: "What I understood": the detected sections, length and voice in plain words, with the uncertain items from extraction phrased as questions the therapist can answer in Step 3. **[call]**
- Checks run silently; on a failed check the coach adds the matching correction to the style and retries, up to three attempts, before showing anything.
- The best attempt is always shown, with a plain label such as "Matches your style: 8 of 9 formatting checks passed", and one sentence naming any failure. The therapist can always continue. **[Patrick]** (shown as a "score"; the wording above is my call, because it must not claim accuracy or confidence.)
- "Try another practice session" switches the fictional session. The app picks the first one automatically. **[call]**
- Generation is cancellable and shows a plain progress message. It does not start while a recording is active. **[call]**

### 7.4 Step 3, make changes
- A text box ("What would you like different?") plus one-tap suggestions (shorter, one sentence per section, include an exact quote when there is one, add, remove or make a section optional). **[call]**
- Each request is interpreted into a small validated change. The app re-runs the same practice session with old and new styles and shows them side by side with "Use this" and "Keep the old one". Nothing changes until "Use this".
- Section changes **[Patrick]**: the coach can remove a section, make a section optional or required, and rename a section. Suggestions "Remove a section" and "Make a section optional" show the current sections as buttons to pick from, so nothing has to be typed. The same actions work from typed requests ("remove Next Actions", "make Risk optional", "rename Client Discussed to Presenting Concerns"). A request naming a section that does not exist gets a question with the real section names as buttons.
- Vague requests ("make it better") get one focused question with answer buttons. Requests the coach cannot make get a plain "I can change length, quotes and sections" with the suggestions again.

### 7.5 Finish
Name field (default "My note style"), a checkbox "Use it for my next sessions" (on by default **[call]**), Save. The style appears in Note Styles under Custom.

### 7.6 Data model
- `prompts` gets a nullable `style_spec` JSON column (new migration). `Prompt.text` holds the compiled prompt; `style_spec` holds the structured preferences. `Prompt` gains an optional `styleSpec`; regenerate the `ts-rs` types.
- Editing a coach-made style's text by hand in the existing editor detaches it: after a plain confirmation, `style_spec` is cleared and the style becomes an ordinary custom style. **[call]**
- Practice runs are never stored as sessions, never counted in usage, and write no note content to logs or telemetry.

### 7.7 Engine (port from `app/coach.py`)
New Rust module `src-tauri/src/coach/`:

| Source function | Port | Notes |
|---|---|---|
| `extract_preferences`, `split_sections` | `extract` | deterministic |
| `validate_preferences`, `compile_prompt`, fixed grounding block | `compile` | grounding block cannot be removed by edits; compiled prompt is about 1.5k characters |
| `check_note` and the nine checks | `checks` | sections present, no repeats, optional omission, length, exact quotes, quote attribution, no names, pronouns, no preamble |
| `revise_for_failures`, correction blocks | `retry` | up to three attempts |
| `interpret_feedback`, `apply_typed_patch` | `interpret` | model returns JSON only; allowed paths are sentence min/max, quote max/required, emphasis, add / remove / rename section, set a section optional or required; no model-written prompt text. Section edits are matched to existing headings in code, tried deterministically before the model is asked, and a style must always keep at least one required section |

Generation must go through the same `chunk_prompt` / `combine_prompt` path real summaries use, not a separate system-plus-user format. The demo used an Ollama system/user pair, which the app does not use, so the compiled prompt must be re-checked inside the app's single-turn template. The compiled prompt also has to behave in the combine pass ("following the format and requirements above"), which short practice sessions never exercise. Add a long-transcript test for that.

Bundled data ported from the hackathon repo: the four sample notes and the nine annotated fixtures (`fixtures.json` supplies names and speaker labels for the checks). The ten library transcripts are not used (no next-step annotations).

### 7.8 Unsupported in this version
Reordering sections, and any change outside the typed list in 7.7 (for example free-form "be warmer"). The coach says so plainly and offers the suggestions.

### 7.9 Calls made after the builder check (all **[call]**, open to change)
- **Speaker labels.** Real transcripts label speakers "You" (microphone) and "Speaker 1, 2, ..." by default (`text/merge.rs`, `text/speakers.rs`), and users can rename them (the app's own tests use "Therapist"). Practice transcripts are used as written ("Therapist:", "Client:"), which is a label a user can choose. The quote-attribution and pronoun checks must not depend on those two words: they read the speaker labels present in the transcript passed in. Add tests that run the checks on a relabelled copy ("You" / "Speaker 1") of a fixture. The compiled prompt must not name roles beyond "the client" and "the therapist" in reporter language.
- **Practice vs real work.** Real summaries always go first. A practice job has no session, is not persisted in the queue, and is cancelled when a recording starts. If a real summary is running or queued, Step 2 shows a plain "Waiting for your other notes to finish" state and runs after. A queued real summary may run between a practice's attempts; each attempt is a separate unit.
- **Saved style text is frozen.** A coach-made style stores its compiled text at save time. Changing the compiler later does not rewrite saved styles. "Adjust with the coach" recompiles from `style_spec` with the compiler version current at that time, and shows the usual side-by-side before anything is saved.

## 8. What is left to the builder
Exact UI copy within the plain-language rules; file layout inside `coach/`; input length limits; sampling values; RAM threshold; ordering of tasks inside each part (Part 1 first).

## 9. Verification
- Port the demo's unit tests (45) to Rust; golden tests for compiled prompts of the starter styles.
- `fake-engines` tests for the practice job, retry, cancel and queue interaction.
- Svelte component tests in the existing `*.svelte.test.ts` style for the three steps, error states and the detach confirmation.
- Real-model run on the 8 GB VM with Gemma 4 E2B (this is also the first check that the model loads in the app): the full flow end to end, a long-transcript combine run, and the built-in styles. Assertions on transcript phrases and structure only.
- Report results as single runs, not benchmarks. The hackathon's own E2B results came from one Ollama run on a VM and passed 6 of 8 formatting checks on one sample; do not treat them as app evidence.
- No claim of clinical accuracy, clinician acceptance or time saved. An unaided walkthrough by a non-technical therapist is the acceptance test for "hand-holding"; pass/fail criteria are agreed with Patrick before it is run.

## 10. Risks and open items
- Gemma 4 E2B may not load through the app's llama.cpp, fit memory alongside speech models on 8 GB, or produce acceptable summaries (section 6.1). This is checked by the first real-model run, not a separate spike.
- Every user downloads a larger model on update, and a Mac that cannot run it loses summaries but keeps recording and transcription. Accepted by Patrick; the hardware-gate message is the mitigation.
- Quality of free-text interpretation on a small model is unproven in-app; validation and deterministic fallbacks limit the damage.
