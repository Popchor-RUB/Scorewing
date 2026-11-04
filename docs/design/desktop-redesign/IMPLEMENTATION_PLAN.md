# Desktop redesign implementation plan

## Start here

Implement the committed Figcraft design in a new session. This checkpoint contains **design artifacts only**; do not assume the application or its dependencies have already changed.

Read these files first:

1. `README.md` beside this plan: the agreed brief and behavioral invariants.
2. `index.html`: the 16-screen gallery; use `screens/split.png` and `screens/merge.png` for the final reviewed layouts.
3. `scorewing-desktop.figcraft`: editable design, four pages. Page 1 contains the main workspaces; page 2 dark/compact variants; page 3 dialogs/results; page 4 foundations.
4. Repository `AGENTS.md` and relevant skills. The user explicitly selected **shadcn/ui + Base UI**, authorizing its adoption in place of HeroUI. Do not ask the library question again.

Preserve Tauri and all existing Rust/export behavior. Native title bars in the mockups are illustrative; keep platform-native window chrome and menus. Score data in the designs is illustrative, never application constants.

## Final design decisions

- Start with Open, recent scores, and file-drop guidance. Opening a score leads to the compact 2×2 tool chooser. Keep explicit tool selection and an All tools return action.
- Use restrained system typography, compact controls, pane separators, fixed export commands, and a status area. Follow system light/dark appearance.
- **Split has three panes:** source-staff selection on the left; generated-staff fallback configuration in the middle; global split mode and automatic slurs on the right. Each source row has a subtitle showing how many **new staves** it will create in the current mode.
- **Every generated Split staff gets an inline fallback dropdown, including the first.** The original source staff is preserved separately. None (silent) is an explicit choice; other options exclude the generated lane's own voice. Do not disable fallback just because it is the first generated row.
- **Merge uses the same flat grouped-table treatment in its middle pane.** Put an Up/Down stem selector in each source row. Keep source selection on the left and whole-group source ordering on the right. Do not duplicate stem selectors in the right pane.
- Practice has ranges, instrument/sound configuration, and mix/renderer panes. At compact widths, use the shown mix/renderer sheet entry point and keep instrument content scrollable.
- Diff selects old/new files, swaps them, and exports marked MusicXML. It has no on-screen score viewer.
- Preserve progress cancellation, retained partial exports, renderer validation, sound search/recents/defaults, native updates, file lifecycle, and keyboard/focus behavior.

## Agent arrangement

Use **one coordinator and at most three workers**. Do not start workers until the coordinator has established the shared component imports and workspace contracts below. Workers must not spawn more agents.

All agents share a checkout. Tell every worker: **You are not alone in the codebase. Edit only your owned files, do not revert others' changes, and report cross-scope changes to the coordinator.** Do not have multiple agents edit `app.tsx`, shared CSS, dependencies, or the test harness.

The coordinator should do one task-directed structural lookup using the codebase-memory skill, check coverage, and provide the findings to workers. The graph project is worktree-specific: choose/index the current checkout, rather than trusting the previous session's project name. Pass each worker the exact files, relevant snippets/contracts, missed coverage ranges, existing tests, and acceptance criteria. Avoid three independent broad repository explorations.

### Ownership

All paths below are relative to `apps/desktop/` unless specified otherwise. New filenames are proposed extraction boundaries, not existing files.

| Owner | Exclusive files/responsibility |
| --- | --- |
| Coordinator | `src/app.tsx`, `src/style.css`, `src/main.ts`, `index.html`, package/lock/config files; shared generated shadcn/Base UI components; shared types/helpers if a real change is necessary; `src/test-support.ts`, `src/app.test.ts`; start/chooser/app shell; integration and final checks |
| Worker A: Split/Merge | New `src/split-workspace.tsx` and `src/merge-workspace.tsx`; focused Split/Merge regression tests in a separately agreed test file. Read existing inline views/model helpers, but do not edit `app.tsx` or `model.ts` |
| Worker B: Practice | `src/practice-view.tsx`, `src/sound-picker.tsx`, `src/practice-export-modal.tsx`; `src/practice.app.test.ts`, `src/sound-picker.test.ts`, and separately agreed practice presentation tests |
| Worker C: Diff/utilities | `src/diff-pair-modal.tsx`, `src/recent-score-button.tsx`, `src/update-control.tsx`, `src/update-prompt.tsx`; new `src/diff-workspace.tsx` if extraction is useful; `src/diff.app.test.ts`, `src/recent-scores.app.test.ts`, `src/updater.app.test.ts`, `src/update-prompt.test.ts` |

Coordinator owns new shared primitives even when a worker requests one. Workers use component-local classes/utility classes and the agreed shared tokens; they do not append competing global CSS or install dependencies. If a listed file/test differs in the next checkout, resolve ownership before work starts.

## Phase 1 — Coordinator establishes the integration boundary

1. Inspect status and existing instructions. Record baseline results for `npm run check`, `npm test`, and `npm run build` from `apps/desktop`. There is no root `package.json`.
2. Read the actual current JSX before choosing extraction seams. Today `app.tsx` owns score sessions, busy guards, native calls, messages, and the inline arrangement/diff views. Keep those responsibilities there.
3. Fetch current official shadcn/Base UI component documentation. Add the selected foundation using npm and the existing frontend lockfile. Add only components the screens need: buttons, checkboxes, inputs/selects, dialogs, sound-search popovers/combobox behavior, and any actually needed sheet/scroll support. Tables can remain semantic HTML. Reuse existing icons/helpers where appropriate.
4. Establish actual shared component exports, semantic tokens, light/dark behavior, focus treatment, and compact pane conventions. Keep generated component source recognizable; do not build a compatibility wrapper reproducing HeroUI's API.
5. Keep HeroUI installed while unmigrated consumers remain. Remove it and its stylesheet import only after all consumers migrate. Do not ship a mixed-library result or duplicate theme systems.
6. Agree the workspace props and error/status ownership with workers, then start the three workers. The coordinator can implement the start/chooser/shell and integration concurrently, within its ownership.

### Existing seams and contracts

These are navigation aids; confirm against the current source before editing.

- `app.tsx` currently has inline `SplitPreview({ part, state, busy, onChange })` and `arrangementView(session: ScoreSession)`. There is no existing `MergeGroups` component.
- The guarded `update(change: (session: ScoreSession) => void)` owns session/busy/disposal checks. Preserve it; presentation components should not bypass it or acquire native invoke access.
- `model.ts` already owns `Part`, `Arrangement`, `fallbackVoice`, `setFallbackVoice`, `addGroup`, `movePart`, and `stemDirection`. Reuse these rather than implementing parallel state rules.
- A sufficient proposed Split/Merge boundary is `parts: Part[]`, `state: Arrangement`, `busy: boolean`, `exporting: boolean`, `onChange: (change: (state: Arrangement) => void) => void`, and `onExport: () => void`. Merge can receive `onCreateGroup: () => void` so the coordinator retains current success/error messages. Adjust this boundary if existing contracts already serve the design; do not add speculative interfaces.
- The coordinator can adapt `onChange` through `update(({ arrangement }) => change(arrangement))`. Keep one owner of notifications and export/file lifecycle.
- `PracticeViewProps` is already exported and covers data, defaults/config/environment, busy/rendering/result state, settings updates, browse/detect/export, and default-setting callbacks. Preserve it where possible.
- `SoundPickerProps` and `PracticeExportModal` already expose the data and callbacks needed for their redesign.
- `DiffFileSelectors` and `DiffPairModal` are both exported from `diff-pair-modal.tsx`. Preserve old/new order, chooser/swap/cancel/compare semantics and error/status handling.
- `RecentScoreButton`, `UpdateControl`, and `UpdatePrompt` should retain their existing external contracts where possible.

## Phase 2 — Worker briefs

### Worker A — Split and Merge

Build the two workspace components against the coordinator's shared primitives and props. Implement the final three-pane Split and flat-table Merge designs; do not copy the superseded per-instrument Split inspector or plain-text Merge stem cells.

Critical details:

- Source-row new-staff counts come from `part.split_plans[state.mode]`, not `voice_count`. Derive the count from actual planned lanes; changing the global mode must update both counts and configuration rows.
- Split fallback storage is keyed by mode, instrument, source voice, and chord tone. Use existing helpers; switching modes must not overwrite the other mode's choices.
- Configure all generated lanes. Display chord-tone labels only in chord mode; explicit-voice mode uses voice labels. Keep None (`null`) distinct from an unset override/default.
- Source selection and settings changes must respect busy state and coordinator guards. Table controls need unique accessible names identifying the source and generated staff.
- Merge group creation clears selection. Assigned sources are disabled in the picker and cannot join a second group. Keep validation and group removal.
- Merge ordering spans the entire group, not separate Up/Down buckets. Explicit stem overrides stay attached to source IDs; implicit defaults follow current index. Use `movePart`/`stemDirection`.
- Every fallback/stem selector stays visible in its row. At 760×560, preserve all three Split panes, combine staff/source metadata within a cell where needed, and scroll content independently with commands visible.

Proof: meaningful regressions for global-mode/count changes, per-lane fallback isolation (including the first generated staff and explicit None), and per-source stem selection/order. Reuse the existing test harness and model fixtures; do not write tests asserting CSS class names or rebuild state helpers.

### Worker B — Practice

Migrate existing presentation while preserving `PracticeViewProps` and all options/config/result semantics. Build ranges, instrument/sound controls, mix/renderer panel, renderer dialog, compact settings entry point, progress/completion, sound search, and partial-results presentation from the gallery.

Keep foreground-track selection independent from background inclusion, foreground level fixed at 0 dB, current level validation/defaults, section selection, and renderer checks. Preserve sound recent/default persistence and keyboard filtering/selection. Active rendering must not dismiss; cancellation must retain completed files and errors/results. Do not add playback, audition, waveforms, ETA, or a new export engine.

Proof: run/adapt existing practice and sound-picker tests. Preserve assertions for configuration validation, progress/cancellation/partial results, keyboard selection and persisted defaults; update locators only where the new accessible UI requires it.

### Worker C — Diff and utility controls

Migrate Diff selectors/pair confirmation, recent-file controls, update controls and update prompts. Extract the Diff workspace only if it gives a clean boundary for coordinator integration. Use the selected shared primitives; keep exported contracts and native behavior stable.

Preserve old/new ordering, swap, cancellation retention, marked-newer export, file existence errors, recent-file keyboard behavior, and updater gating when a score is open or work is busy. Keep both source files intact. Diff success remains an export status; do not implement a score/diff viewer.

Proof: run/adapt owned Diff, recent-score, and updater tests. Coordinate integration failures with the `app.tsx` owner instead of editing that file.

## Phase 3 — Coordinator integrates and verifies

1. Wire workspaces into the existing lifecycle without duplicating state, render triggers, errors or export callbacks. Keep focus transfer when entering/leaving tools and preserve the chosen tool's state according to current behavior.
2. Finish native Open/Close/recent integration, start screen, chooser, status/error/warning presentation, theme initialization and preference changes. Do not hard-code design sample paths/counts, Ctrl/Cmd labels, or fake native window buttons.
3. Confirm keyboard-only operation, checkbox-versus-inspector selection clarity, dialog focus trapping/restoration, status announcements, disabled/busy controls, and reduced-motion behavior.
4. Confirm every control is reachable at 1100×760 and 760×560 in both appearances. Scroll panes rather than the whole app where the design shows panes. Check empty, loaded, invalid, busy, cancelling, complete and partial-failure states.
5. Search frontend source/config for remaining HeroUI imports/usages and obsolete styles. Remove the old dependency/style imports once migration is complete. Preserve useful existing tests and pure-state/storage/progress coverage.
6. Run the final frontend gates once integrated, fixing relevant failures:

   ```sh
   cd apps/desktop
   npm run check
   npm test
   npm run build
   ```

   A worker may run owned tests directly, for example:

   ```sh
   node --experimental-strip-types --test src/practice.app.test.ts src/sound-picker.test.ts
   ```

   Do not have all workers run the complete suite repeatedly while integration is in flight. Preserve behavioral assertions; do not delete failing tests to accommodate new markup.
7. Use available permitted runtime/native tooling to compare the running app with the design and check resizing and menus. Figcraft screenshots are design proof, not application-runtime proof. Native checks use `npm run tauri dev` when the environment supports it. No backend changes are planned; Rust tests become necessary if that scope changes.
8. Review the final diff and report what changed, the commands/results, and any platform-specific checks still unavailable. Stop when the agreed redesign and regression proof are complete; do not add new product features or speculative infrastructure.

## Handoff requirements for each worker

Return only: owned files changed; implemented behavior; exact checks/results; remaining issues or coordinator actions. Flag cross-scope needs early. No dependency changes, commits, PRs, or unrelated cleanup unless the coordinator explicitly assigns them.

## Environment notes from the design session

- The current implementation uses React 19, Tailwind 4, HeroUI 3.2.6 and a Tauri desktop host. Recheck exact installed versions before changing dependencies.
- Current frontend commands are `npm run check`, `npm test`, and `npm run build` from `apps/desktop`; the lockfile is `apps/desktop/package-lock.json`.
- T3's HTML preview reported missing host browser libraries during design. Figcraft rendering worked; the gallery's asset integrity, JavaScript selection/navigation, representative text/control contrast and font availability were checked. Do not treat the unavailable HTML preview as a runtime pass.
- Native macOS/Windows audio rendering was already documented as unverified; do not claim this UI work proves it.
- Git metadata for this worktree lives outside its writable sandbox. The design session used an approved escalated Git command to inspect the worktree. Prefer the normal allowed path first in the next environment, and request escalation only where its sandbox requires it.

## Completion condition

All four tools and their existing states match the agreed desktop structure, shadcn/Base UI replaces HeroUI throughout the frontend, native/lifecycle/export invariants remain intact, and the relevant typecheck/tests/build and available runtime checks pass. Any unavailable platform check is reported explicitly. No UI implementation belongs in the design-only checkpoint containing this plan.
