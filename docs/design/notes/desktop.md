# ScoreWing desktop redesign

Status: implemented in the desktop frontend using application-owned shadcn/Base UI primitives. The [consolidated Figcraft source](../scorewing.figcraft) is the visual design reference; the [historical implementation plan](desktop-implementation-plan.md) records the original scope and behavioral invariants.

Open [the unified gallery](../index.html) for every screen, [the Figcraft source](../scorewing.figcraft) for editable layers, or [the screen inventory](../README.md) for individual PNG exports. Frames 01–16 capture the original desktop redesign.

The approved [MuseScore import design](musescore-import.md) adds frames 17–19 to the editable document. The [unified gallery](../index.html) covers the implemented Split/Merge visibility defaults and export mark warnings.

The approved and implemented [export destination design](export-destinations.md) adds page 05, frames 20–23, for tool-specific filenames, direct practice section folders, and overwrite/clear confirmation. The [unified gallery](../index.html) shows all four states.

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

## Instrument visibility groups — November 5 revision

Status: updated in the existing Figcraft source and affected exports; approved for implementation on November 6, 2026. This revision supersedes earlier visibility/default statements where they differ.

- Split and Merge group their left source lists, and Practice groups its middle instrument table, under Visible instruments followed by Hidden instruments. Both sections are always expanded, empty sections are omitted, and source-score order is preserved within each group. Grouping must preserve original metadata indices and instrument identity.
- Remove the per-entry Hidden in score and Hidden in source labels. Group headings convey source visibility. Retain multi-staff eligibility explanations and disabled, unchecked Split/Merge controls.
- On a newly opened score, Split checks every eligible visible instrument for every supported source format; hidden instruments start unchecked. Merge starts with every instrument unchecked. Practice checks both foreground track creation and background inclusion for visible instruments; both start unchecked for hidden instruments.
- These are new-score defaults. Remember each tool's manual choices when navigating between tools. Select all retains its existing scope, including eligible hidden sources; grouped and unsupported Merge sources remain ineligible.
- Each Merge source occupies one compact single-line row: checkbox, instrument name, and a smaller Grouped indicator immediately next to the name when assigned. Do not add voice-count or separate status sublines. Unsupported multi-staff instruments retain a smaller inline 2 staves · Not supported explanation, disabled checkbox, and accessible description.
- Merge's selected count and Create merge group button form a fixed footer below the scrolling source list. They remain visible while a long instrument list scrolls. Export commands retain their existing fixed placement.
- Practice keeps the instrument name as a separate heading, followed by two labeled rows. Create practice track controls foreground track creation and its foreground sound; Include in background controls accompaniment and its background sound. Each checkbox is on the left of its label, with the associated sound input and Set Default action aligned to the right.
- Both Practice sound pickers remain visible when their checkbox is unchecked. Disable the corresponding input and Set Default action, preserving and displaying the saved sound; do not hide the input or clear the choice. Enable each role independently. Near Instruments, explain that checking Create practice track creates one MP3 per selected range and background instruments accompany the other tracks.
- Use the standard shadcn Base UI base-nova editable ComboboxInput appearance with a trailing chevron, ComboboxContent, searchable List/Item results, selected checkmarks, and normal recent/all sound groups. Show each preset once, excluding recent preset IDs from the all-sounds list. The input itself is editable; do not substitute a button-triggered popover with a separate search box. Keep filtering, no-results feedback, recent sounds, arrow-key navigation, Enter selection, Escape dismissal, and focus behavior. Frame 13 shows the focused input and open results; unchecked hidden rows illustrate visible disabled inputs.
- Frame 04 intentionally illustrates an already-created merge group; frame 18 illustrates the empty initial Merge state. Main Practice examples use Soprano and Alto as visible instruments and Tenor and Bass as hidden instruments. Compact views illustrate clipping inside the scrolling list rather than expanding the window.

## Implementation invariants

- Keep native OS title bars and existing native menus. The illustrated title bar is a reference, not a custom-chrome requirement. Show platform-appropriate shortcut labels.
- Keep native file/folder pickers, score release behavior, recent-file persistence, one-/two-file drops, updater behavior, and busy-state locks.
- Split uses three panes at both window sizes. Source selection on the left shows the number of new staves each source will create, derived from its current split plan and updated when the global mode changes. The middle pane shows every generated staff with its own inline fallback dropdown, including the first generated staff. Split mode and automatic fallback slurs remain global settings in the right pane. The untouched original staff is preserved separately. Fallback choices are None (silent) and other source voices, excluding the row's own voice; a single-voice instrument can offer only None (silent).
- Merge sources cannot belong to multiple groups. Creating a group clears selection; assigned sources are disabled in the source picker. The middle pane uses the same flat grouped-table treatment as Split and places an Up/Down stem selector in every source row. The right pane retains source ordering without duplicating the stem selector. Reordering spans the whole group; explicit stem overrides stay attached to instrument IDs, while implicit directions default by index. Preserve these rules and group removal.
- Practice starts each newly opened score with Whole song and every rehearsal section selected. The top of the Ranges pane uses the Split design's blue text actions, Select all · Clear. Both include Whole song, work without rehearsal marks, and are disabled during checking or rendering. Choices persist when switching tools within the same score. Retain independent foreground-track and background inclusion, sound defaults, mix validation, and renderer checks. The light, dark, and compact Figcraft screens and gallery exports show this default state.
- Keep MuseScore 4.7.x and matching MS Basic validation; retain the reminder to close MuseScore during rendering.
- Sound search retains filtering, recent sounds, arrow-key navigation, Enter selection, Escape dismissal, and focus restoration.
- Active rendering cannot dismiss its dialog. Cancellation remains explicit; keep completed files and show partial outcomes and errors.
- Native export destinations and non-destructive output behavior remain unchanged.
- Diff exports marked MusicXML. The concept adds no notation viewer, playback, waveform, change-count display, or project-file format.
- Use semantic labels and real controls; checkbox state must be distinct from inspector selection. Preserve visible keyboard focus, dialog focus trapping, status announcements, and reduced-motion preference.
- Scroll content inside panes. Keep primary commands visible. At compact Practice widths, place mix/renderer controls in an accessible sheet.

## Expected implementation scope

Review the desktop frontend dependencies and styling; app shell/start/chooser; Split/Merge configuration; Practice view, sound picker, settings and progress/completion surfaces; Diff selectors/dialog; recent-score controls; associated integration tests and keyboard/focus behavior. Reuse the existing Tauri/Rust commands and export logic.

## Verification

The design handoff checked the editable Figcraft layer tree, rendered workspace and state screenshots, minimum-size variants, fonts, colors, gallery assets, and gallery JavaScript.

Implementation checks: `npm run check`, `npm test` (14 test files), `npm run build`, and `git diff --check` pass. Regression coverage includes mode-dependent Split counts and isolated fallback choices, Merge stems and whole-group ordering, Practice settings/defaults/cancellation/partial results, sound keyboard selection, recent files, Diff lifecycle, and updates.

Browser runtime checks pass in rootless Podman using `mcr.microsoft.com/playwright:v1.64.1-noble`, with networking disabled and the built frontend mounted read-only. Chromium checked 1100×760 and 760×560 in light and dark appearance, capturing 54 screenshots across the start screen, chooser, four workspaces, sound search, renderer settings/errors, progress, cancellation, partial results, completion, and compact mix sheet. Checks cover viewport bounds, readable inline fallback selectors, independent pane scrolling, fixed commands, keyboard selection, dialog focus trapping/restoration, busy-state guards, and live system-theme changes. This review fixed compact Split table sizing and tightened Practice/dialog layouts.

Browser checks mock the Tauri file/renderer bridge. Real native OS menus, screen-reader operation, and macOS/Windows audio rendering still require platform checks. The native development command cannot bind its Vite port in the sandbox, and GTK/WebKit development packages are unavailable; no host libraries were installed.
