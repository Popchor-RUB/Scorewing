# ScoreWing design

[scorewing.figcraft](scorewing.figcraft) is the single editable source of truth for the desktop UI. It contains all 26 existing screen/state frames on six pages, including foundations, dark and compact variants, MuseScore import, export destinations, and the update window. The update-window layers and embedded application icon were merged into the main document using the Figcraft MCP.

Open [the gallery](index.html) to browse every frame, or select an individual PNG below. `screens/` contains exports from the consolidated document: desktop frames at 1× and update-window frames at 2×. Frame numbers are unique across the document and prefix export filenames.

## Updating the design

1. Open `docs/design/scorewing.figcraft` with the Figcraft MCP and edit the relevant frames for every user-requested design change. Add new screens, states, and variants to this document.
2. Save the document, inspect the affected layers, and visually check the rendered result. Export every affected frame to its matching PNG in `docs/design/screens/`; add new exports and remove obsolete ones. Preserve the export scales listed below unless deliberately changing them.
3. Update this inventory, `index.html`, and relevant behavioral notes in the same change. Each screen/state frame must have exactly one current PNG and gallery entry. Derive exports from Figcraft; do not edit PNGs or maintain another design source.
4. Show the updated design and obtain user approval before frontend implementation, as required by [AGENTS.md](../../AGENTS.md). Keep source, exports, gallery, and notes synchronized through later design revisions.

This consolidation preserves the existing screen layouts and behavioral requirements. Frames 17–19 describe the later MuseScore defaults and export warnings; frames 20–23 describe the later export naming and destination behavior. Use those feature notes for those behaviors when reading the original desktop handoff. Illustrated score names, paths, counts, and version numbers are sample data.

## Behavioral notes and history

- [Desktop design brief and invariants](notes/desktop.md)
- [MuseScore import and export warnings](notes/musescore-import.md)
- [Export names and practice destinations](notes/export-destinations.md)
- [Update window](notes/update-window.md)
- [Historical desktop implementation plan](notes/desktop-implementation-plan.md) — records the original migration handoff; implementation and verification status is recorded in the feature notes.

These notes preserve decisions and historical verification evidence; the editable visual design lives in `scorewing.figcraft`.

## Screen inventory

| Frame | Figcraft page | Layer ID | PNG | Scale |
| --- | --- | --- | --- | --- |
| 01 · Start | 01 · Desktop workspaces | `7` | [01-start.png](screens/01-start.png) | 1× |
| 02 · Tool chooser | 01 · Desktop workspaces | `47` | [02-tool-chooser.png](screens/02-tool-chooser.png) | 1× |
| 03 · Split instruments | 01 · Desktop workspaces | `99` | [03-split.png](screens/03-split.png) | 1× |
| 04 · Merge instruments | 01 · Desktop workspaces | `184` | [04-merge.png](screens/04-merge.png) | 1× |
| 05 · Practice Tracks | 01 · Desktop workspaces | `279` | [05-practice.png](screens/05-practice.png) | 1× |
| 06 · Diff versions | 01 · Desktop workspaces | `423` | [06-diff.png](screens/06-diff.png) | 1× |
| 07 · Practice Tracks — Dark | 02 · Dark and compact | `485` | [07-practice-dark.png](screens/07-practice-dark.png) | 1× |
| 08 · Tool chooser — Dark | 02 · Dark and compact | `631` | [08-tool-chooser-dark.png](screens/08-tool-chooser-dark.png) | 1× |
| 09 · Minimum window | 02 · Dark and compact | `683` | [09-split-compact.png](screens/09-split-compact.png) | 1× |
| 10 · Export in progress | 03 · Dialogs and feedback | `765` | [10-export-progress.png](screens/10-export-progress.png) | 1× |
| 11 · Export complete | 03 · Dialogs and feedback | `921` | [11-export-complete.png](screens/11-export-complete.png) | 1× |
| 12 · Renderer settings | 03 · Dialogs and feedback | `1083` | [12-renderer-settings.png](screens/12-renderer-settings.png) | 1× |
| 13 · Sound search | 03 · Dialogs and feedback | `1254` | [13-sound-search.png](screens/13-sound-search.png) | 1× |
| 14 · Partial export | 03 · Dialogs and feedback | `1412` | [14-partial-export.png](screens/14-partial-export.png) | 1× |
| 15 · Practice Tracks — Minimum window | 02 · Dark and compact | `1570` | [15-practice-compact.png](screens/15-practice-compact.png) | 1× |
| 16 · Design foundations | 04 · Design foundations | `1690` | [16-foundations.png](screens/16-foundations.png) | 1× |
| 17 · MuseScore import — Split defaults | 01 · Desktop workspaces | `2255` | [17-musescore-split.png](screens/17-musescore-split.png) | 1× |
| 18 · MuseScore import — Merge defaults | 01 · Desktop workspaces | `2368` | [18-musescore-merge.png](screens/18-musescore-merge.png) | 1× |
| 19 · Export completed with mark warnings | 01 · Desktop workspaces | `2529` | [19-export-warning.png](screens/19-export-warning.png) | 1× |
| 20 · Default export filenames | 05 · Export destinations | `3106` | [20-export-filenames.png](screens/20-export-filenames.png) | 1× |
| 21 · Practice tracks — direct output | 05 · Export destinations | `2674` | [21-practice-direct-output.png](screens/21-practice-direct-output.png) | 1× |
| 22 · Existing tracks — overwrite | 05 · Export destinations | `2818` | [22-overwrite.png](screens/22-overwrite.png) | 1× |
| 23 · Existing tracks — clear folder | 05 · Export destinations | `2962` | [23-clear-folder.png](screens/23-clear-folder.png) | 1× |
| 24 · Update available · Light | 06 · Update window | `3177` | [24-update-available-light.png](screens/24-update-available-light.png) | 2× |
| 25 · Update available · Dark | 06 · Update window | `3189` | [25-update-available-dark.png](screens/25-update-available-dark.png) | 2× |
| 26 · Installation disabled · Light | 06 · Update window | `3201` | [26-update-installation-disabled.png](screens/26-update-installation-disabled.png) | 2× |
