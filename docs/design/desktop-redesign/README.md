# ScoreWing desktop redesign

Status: design checkpoint requested for commit. Application implementation and dependency migration are deferred to a new session; follow `IMPLEMENTATION_PLAN.md` for coordinator and subagent ownership.

Open `index.html` for the 16-screen gallery, or `scorewing-desktop.figcraft` for editable layers. The `screens/` folder contains PNG exports.

## Agreed brief

- Cross-platform desktop utility with balanced density and a quiet, neutral visual tone.
- Follow the system light/dark preference.
- Start with Open, recent scores, and a file-drop hint.
- After opening a score, show a compact 2×2 tool chooser.
- Give Split, Merge, Practice Tracks, and Diff equal design attention.
- Use lists, inspectors, compact command bars, pane separators, and fixed export commands.
- Preserve the existing 1100×760 default window and 760×560 minimum.
- Review the Figcraft concept before application changes.

The user selected **shadcn/ui + Base UI** after reviewing the migration and maintenance tradeoff. Component source copied into the repository becomes application-owned code; upstream accessibility and behavior fixes need deliberate updates.

Reference evidence checked November 4, 2026:

- [T3 Code shadcn configuration](https://github.com/pingdotgg/t3code/blob/main/apps/web/components.json) and [web dependencies](https://github.com/pingdotgg/t3code/blob/main/apps/web/package.json): shadcn/Base UI.
- [Stably Orca shadcn configuration](https://github.com/stablyai/orca/blob/main/components.json) and [dependencies](https://github.com/stablyai/orca/blob/main/package.json): shadcn/Radix.
- [shadcn open-code model](https://ui.shadcn.com/docs), [Base UI overview](https://base-ui.com/react/overview/about).

These references use web UI components in desktop hosts. This design retains ScoreWing's existing Tauri host.

## Screens and ownership

| Screens | Design responsibility |
| --- | --- |
| Start, tool chooser | Recent files, native Open, drag/drop, compact operation selection |
| Split | Three panes: source-staff selection with new-staff-count subtitles on the left; per-generated-staff fallback configuration in the middle; global options on the right |
| Merge | Available sources on the left; flat group tables with per-source Up/Down stem selectors in the middle; source ordering on the right |
| Practice Tracks | Rehearsal ranges, enabled tracks, foreground/background sounds, mix and renderer settings |
| Diff | Older/newer file selection, swap, export of marked newer MusicXML |
| Dark variants | Shared semantic colors and control treatment |
| Minimum-window variants | Scrollable content, persistent commands, compact mix/renderer access |
| Export and settings states | Progress, cancellation, success, renderer paths, sound search, retained partial results |
| Foundations | Color references, dimensions, component states, implementation invariants |

Score names, paths, voices, rehearsal ranges, and export counts are illustrative.

## Implementation invariants

- Keep native OS title bars and existing native menus. The illustrated title bar is a reference, not a custom-chrome requirement. Show platform-appropriate shortcut labels.
- Keep native file/folder pickers, score release behavior, recent-file persistence, one-/two-file drops, updater behavior, and busy-state locks.
- Split uses three panes at both window sizes. Source selection on the left shows the number of new staves each source will create, derived from its current split plan and updated when the global mode changes. The middle pane shows every generated staff with its own inline fallback dropdown, including the first generated staff. Split mode and automatic fallback slurs remain global settings in the right pane. The untouched original staff is preserved separately. Fallback choices are None (silent) and other source voices, excluding the row's own voice; a single-voice instrument can offer only None (silent).
- Merge sources cannot belong to multiple groups. Creating a group clears selection; assigned sources are disabled in the source picker. The middle pane uses the same flat grouped-table treatment as Split and places an Up/Down stem selector in every source row. The right pane retains source ordering without duplicating the stem selector. Reordering spans the whole group; explicit stem overrides stay attached to instrument IDs, while implicit directions default by index. Preserve these rules and group removal.
- Practice retains whole-song/section selection, independent foreground-track and background inclusion, sound defaults, mix validation, and renderer checks.
- Keep MuseScore 4.7.x and matching MS Basic validation; retain the reminder to close MuseScore during rendering.
- Sound search retains filtering, recent sounds, arrow-key navigation, Enter selection, Escape dismissal, and focus restoration.
- Active rendering cannot dismiss its dialog. Cancellation remains explicit; keep completed files and show partial outcomes and errors.
- Native export destinations and non-destructive output behavior remain unchanged.
- Diff exports marked MusicXML. The concept adds no notation viewer, playback, waveform, change-count display, or project-file format.
- Use semantic labels and real controls; checkbox state must be distinct from inspector selection. Preserve visible keyboard focus, dialog focus trapping, status announcements, and reduced-motion preference.
- Scroll content inside panes. Keep primary commands visible. At compact Practice widths, place mix/renderer controls in an accessible sheet.

## Expected implementation scope

Review the desktop frontend dependencies and styling; app shell/start/chooser; Split/Merge configuration; Practice view, sound picker, settings and progress/completion surfaces; Diff selectors/dialog; recent-score controls; associated integration tests and keyboard/focus behavior. Reuse the existing Tauri/Rust commands and export logic.

## Verification for this design stage

The editable Figcraft layer tree, rendered workspace and state screenshots, minimum-size variants, available fonts, text/control colors, gallery assets, and gallery JavaScript are checked during handoff. Actual desktop behavior, screen-reader operation, dependency compatibility, and regression tests must be verified during implementation.
