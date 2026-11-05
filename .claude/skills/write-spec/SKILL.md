---
name: write-spec
description: Use when writing a new spec for scribular-tauri, a feature, parity item or change that will be handed to cwf init --spec.
---

# Write a spec

## Purpose

Write one spec per feature, using `plans/SPEC_TEMPLATE.md` as the format. Always
start with an interview before drafting.

## Read first

Before asking anything, read the relevant code to understand Tauri state today,
`REVIEW.md`, and neighboring specs in `plans/` and `plans/parity/`. Never ask
what the code or existing specs already answer.

## Interview

Load and follow `superpowers:brainstorming` for the interview, approach options,
and size call. Apply these explicit overrides:

- The spec is the design doc. Do not write `docs/superpowers/specs/...`.
- After approval, do not invoke `superpowers:writing-plans`; hand off through
  `cwf init` below.
- Keep brainstorming's reviewer/builder-check pass on the finished spec and its
  rule that nothing is built before Patrick approves.

## Draft

Copy `plans/SPEC_TEMPLATE.md` and fill every section. Delete optional sections
that are empty, leave no placeholders, and remove the template-notes comment.
Write Intent through Design notes first. Write Summary and User workflow last,
after the spec is settled, so they reflect it accurately. Keep both concise and
jargon-free. The workflow is Mermaid with 12 nodes or fewer; mark new or changed
steps.

## Paths

- Parity: `plans/parity/NN-<slug>.md`.
- Everything else: `plans/YYYY-MM-DD-<slug>-spec.md`.
- Start the header with `Status: draft for cwf; Date: YYYY-MM-DD`.

## Dependencies

Use the template's light “Dependencies and security check.” Do not create a
separate `DEPENDENCY_REVIEW` document or apply a publish-date cutoff. Do not add
an approval gate unless there is a concrete material issue: incompatible
license, known exploitable vulnerability affecting our use, or functional
failure.

## Completion check

Before asking for approval, confirm:

- Every R has at least one AS, and every AS is tagged automated or manual.
- No TBD or placeholder text remains.
- Summary and workflow match the requirements.
- Every Decision for Patrick includes a default.
- Size and PR split are present.

## Approval and hand-off

Wait for Patrick's approval of the spec. Then commit it on a feature branch and
hand it to the coding-task flow with:

`cwf init --repo-dir <checkout> --spec <spec-path> --reviewer <cubic|knightwatch>`

Ask which reviewer to use if one was not stated. Do not start implementation in
the spec session.
