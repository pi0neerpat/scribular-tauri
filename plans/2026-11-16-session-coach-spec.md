# Session Coach — Tauri spec

- **Status:** started 2026-11-16. Only the exception below is filled in; the other sections are intentionally empty.
- **Design:** scribular-docs `design/session-coach/README.md` (approved Option 3: always-on Coach input below the session note).
- **Related spec:** `plans/2026-11-07-note-style-coach-spec.md`

## Summary

## User workflow

## Intent (why users need it; who)

## Tauri state today (what exists, what is missing, with file refs)

## Goals / Non-goals / Anti-goals

### Exception: real session notes may update a note style (Patrick, 2026-11-16)

The Note Style Coach spec puts "Improving a style from feedback on a real session's note" out of scope, and its examples and practice notes are fictional. **"Also use this for my style" is a new feature and is the one deliberate exception to that rule.** After the therapist accepts Coach changes to a real session note, the Coach may offer to carry those changes into the session's note style.

Implement this carefully:

- The style may take only reusable writing instructions derived from the accepted edits (for example length, sections, bullets, quote use). It must never contain clinical facts, names, quotes or any other content from the client's note or transcript.
- It happens only when the therapist explicitly chooses it, after previewing the style change.
- Style updates are never retroactive: they affect only notes written after the change. Existing notes change only when the therapist edits that note or accepts a Coach change on it.
- Everything stays on the device, as in the Note Style Coach spec.

## Requirements (numbered R1..)

## Acceptance scenarios (numbered AS1..; Given/When/Then; each tagged automated or manual)

## Design notes (modules, commands/events, data model + migration, UI components)

## Dependencies and security check

## Models

## Out of scope

## Depends on / interacts with (other specs by slug)

## Coordination notes

## Decisions for Patrick (with recommended default)

## Size (S/M/L, rough days) and suggested PR split if > ~1 PR
