# HeroUI single-score workspace

## Summary

Rebuild Scorewing's desktop UI with the installed HeroUI 3.2.6 components. Use a warm cream-and-forest-green design, clear editing cards, and a prominent export footer. Edit one score at a time: remove the queue, file sidebar, and switching between retained documents.

This plan replaces the earlier queue-based UI plan. The implementation is complete; verification is recorded below.

## Single-score lifecycle

- Show an empty workspace with Open score and a single-file drop target.
- When a score is loaded, show its name with Open another score and Close score actions.
- Close score releases the native document session, clears editing choices and score-specific messages/results, and returns to the empty workspace. Explain near the action that editing choices are discarded; source files and exported files remain intact.
- Open another score uses a single-file picker. Cancelling keeps the existing score and its editing choices. Inspect the replacement before committing the switch; on failure retain the current score and show the error. Release unused native sessions during replacement and failure cleanup.
- Change the native picker command from open_files returning a path array to open_file returning an optional single path. Update its frontend caller, Tauri registration, and test mocks together. Reuse inspect_file and release_document.
- Apply the same replacement behavior to a single dropped file. Reject multiple-file drops with a message asking for one score; do not silently choose a file.
- Disable file opening, closing, and replacement while a file operation or export is in progress. Retain practice-export cancellation. Prevent stale progress or asynchronous completions from updating a different score.
- Keep renderer configuration and recent instrument history across score changes. Reset arrangement and practice choices for each newly opened score.

## React views and visual refresh

- Use one React root, TSX views, and typed callbacks instead of HTML-string rendering and delegated events. Keep native command orchestration in the existing controller, simplified to one current score; reuse model transformations and validation.
- Use HeroUI Buttons, Selects, Checkboxes, RadioGroups, TextFields, NumberFields, Cards, and progress/status components for the corresponding UI.
- Build a single-column workspace with a compact score header, editing cards, and a persistent export footer. Adapt controls and cards to narrow windows without horizontal overflow.
- Use HeroUI theme tokens and scoped layout styles; remove conflicting global control styling and the separate smoke-test UI.
- Preserve split previews and fallback choices, merge grouping/reordering and stems, practice configuration, renderer detection, validation, warnings, progress, cancellation, and partial export results.
- Preserve accessible labels, keyboard focus, contrast, and scroll position during updates. Register native listeners once and clean them up on unmount.

## Searchable practice instruments

- Build one HeroUI ComboBox used for both foreground and background sound selectors.
- Focus opens the complete available catalog even with an existing selection. Select the displayed label for replacement; only user edits become the search query.
- Filter names case-insensitively and show bank/program alongside names to distinguish duplicates. Support arrow keys, Enter, Escape, and mouse selection using HeroUI behavior.
- An unfiltered open shows Recently selected first, with up to five distinct instruments, newest first, followed by the remaining catalog without duplicate rows. Hide recents while searching and restore them when search is cleared.
- Share recents across all selectors and scores, and persist canonical bank/program IDs in versioned localStorage. Record explicit selections only; reselecting an instrument moves it first. Defaults and typing do not affect recents.
- Validate stored history, hide instruments absent from the current catalog, and fall back to in-memory recents if storage is unavailable.
- Commit only valid catalog selections. Abandoning a search restores the committed label. Preserve clearing a sound, unavailable-preset indicators, empty-result feedback, and existing export validation.

## Verification and implementation split

- Adapt UI tests to React using the established Vite/happy-dom setup; retain model tests.
- Replace queue tests with open, close, reopen, successful replacement, cancelled picker, failed replacement, native-session cleanup, multiple-file drop rejection, and busy-state checks. Verify no score-specific settings leak to a new score, while renderer settings and recents survive.
- Retain split/merge and practice-export regression coverage, including native payloads, validation, progress, cancellation, and partial results.
- Test picker focus, filtering, duplicate names, keyboard interaction, dismissal, clearing, disabled/empty states, and focus stability during updates.
- Test recent ordering, deduplication, five-item capacity, sharing, persistence after remount, malformed storage, storage failure, and unavailable presets.
- Run desktop tests and the production build, focused Rust checks for the picker change, and a browser review at desktop/narrow widths with keyboard-only operation. Verify native dialogs and rendering in a Tauri runtime when available.
- Delegate the instrument picker and its tests after agreeing its props. Keep the single-score controller, React migration, and theme under one owner; use a separate integration review.
- After implementation, ask the user to commit and suggest: feat(ui): rebuild single-score workspace with HeroUI.

No new dependencies, theme switching, unrelated backend changes, or export-format changes are planned. Previous baseline: all 10 desktop tests and the production build passed, with existing HeroUI use-client warnings. Native-runtime behavior remains to be verified during implementation.

## Implementation verification

- Desktop frontend: `npm test` passes all 12 tests; `npm run build` passes TypeScript and the production bundle. The reported SoundPicker `id` prop error is fixed, with IDs supplied through HeroUI fields to retain accessible labels.
- Native desktop library: `cargo test -p scorewing-desktop --lib --locked --offline` passes all 15 tests in the prepared devcontainer. The single-file picker command compiles there.
- WebKit browser previews at 1280px and 390px show no JavaScript errors or horizontal overflow. The visual check covers split/practice views and the open instrument dropdown with recent sounds, using mocked native commands.
- Native macOS dialogs, macOS packaging, and actual MP3 rendering were not exercised in this environment.
