# App Shell P1 Build Plan

**Goal:** Replace page navigation with a persistent sidebar and local session search.

**Architecture:** Keep session list and selection in `SessionHistory`, which becomes the shell layout. Search stays inside the encrypted SQLite store and returns existing list items. App navigation continues to own leave guards and model lock.

**Spec:** `plans/2026-11-16-app-shell-spec.md`, P1 execution notes.

- [x] Add `search_sessions` store query, Tauri command, and typed API wrapper; test fields, ordering, and literal matching.
- [x] Move session list into the persistent sidebar; add date groups, row markers, contextual actions, search errors, and stable selection; test state transitions.
- [x] Route Record, Note Styles, Settings, and License through the main content while preserving navigation guards.
- [x] Run automated checks, commit and push. Record AS1 and AS4 as manual-only for Patrick and hand off to verify.
