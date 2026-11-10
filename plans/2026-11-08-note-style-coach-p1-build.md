# Note Style Coach P1 Build Plan

> Build the approved model-swap scope from `plans/2026-11-07-note-style-coach-spec.md`.

1. Replace the manifest model and llama parameters; pin artifact URL, size, hash and license, and add manifest/parameter checks.
2. Migrate stale stored summary model ids to the new default and remove the frontend model picker while retaining backend selection commands.
3. Share physical-RAM detection, gate only the LLM download, expose `SummaryAvailability`, and keep recording/transcription usable on unsupported or load-failed Macs.
4. Delete the old model only after the required success conditions (or immediately on a gate-failing Mac); test interrupted, failed and successful update paths.
5. Run the P1 automated checks, commit and push, transition to `verifying`, and hand off the manual real-app scenarios.

**Out of scope:** coach engine, style persistence, practice jobs and coach UI (AS12–AS32).
