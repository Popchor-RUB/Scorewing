# MuseScore import for Split and Merge

Status: requirements and Figcraft design approved on November 4, 2026; implemented in the desktop frontend, conversion pipeline, and shared arranging engine.

Open [the unified gallery](../index.html). Editable frames 17–19 are on the Desktop workspaces page in [the single Figcraft source](../scorewing.figcraft). Their PNG exports and layer IDs are listed in [the screen inventory](../README.md).

## Agreed behavior

- For MuseScore files opened in Split or Merge, convert the full score to MusicXML. Include hidden instruments and preserve the immutable original file snapshot.
- List every original instrument in source order. Eligible visible instruments start checked; hidden instruments start unchecked. Source visibility must account for instrument and staff visibility, rather than dropping hidden content during conversion.
- Instruments with multiple original staves remain listed with disabled, unchecked checkboxes and a visible explanation. They remain unsupported for transformation even if only one of their staves is visible. Their presence must not prevent other instruments from being inspected, split, merged, or exported.
- Preserve tempo text, metronome markings, playback tempo changes, and rehearsal marks at their original musical positions. Preserve score-wide marks even if their carrier is hidden, unchecked, or unsupported for transformation. Keep them effective in generated visible results without introducing duplicate playback events from identical score-wide markings.
- If required marks cannot be preserved, allow export and show a persistent warning identifying the affected mark and musical position. Do not claim complete preservation when verification is incomplete. A conversion failure that prevents a usable score remains an error.
- Both Split and Merge retain every source instrument in the export with its original visibility. Generated split staves and merged instruments are visible, including results made from hidden sources. Unchecked means do not transform; it does not mean omit from export.
- Remember selections separately for Split and Merge while the same score is loaded. Initialize each tool once from original visibility; a newly opened file starts fresh.

## UI design

Keep the existing three-pane arrangement, semantic colors, typography, and application-owned shadcn/Base UI controls. No new UI dependency is needed.

| Frame | State | Review focus |
| --- | --- | --- |
| 17 | Split defaults | Two visible sources checked; hidden source unchecked; multi-staff piano disabled; all originals retained |
| 18 | Merge defaults | Same visibility defaults; explicit group creation; export disabled until a valid group exists |
| 19 | Export with warnings | Export succeeds; warning names missing tempo text and rehearsal mark with measure and beat |

Instrument subtitles distinguish source visibility from eligibility. Disabled multi-staff rows expose the reason as text and an accessible description; color alone does not convey state. Select all and Clear operate only on eligible, unassigned sources. Grouped sources retain the existing restriction against appearing in multiple merge groups. Removing a group makes those sources eligible again without resetting unrelated selections.

The illustrated warning entries are examples, not evidence of actual conversion loss. Warning details must remain available after export and be announced accessibly. A long warning list scrolls without hiding the export commands. Reuse existing status and export-result handling where it fits; avoid an additional confirmation dialog.

Maintain the existing 1100×760 default and 760×560 minimum window sizes, independently scrolling panes, visible keyboard focus, busy-state guards, and system light/dark appearance. The current review exports show the default light appearance; compact and dark runtime behavior require verification during implementation.

## Implementation scope

1. Extend MuseScore archive inspection and conversion at their existing ownership points. Preserve all original parts and staff visibility; carry stable source identity, staff count, eligibility, and mark metadata through conversion. Avoid accidental changes to Practice rendering, which shares archive utilities.
2. Extend core inspection to tolerate and retain unsupported multi-staff parts without permitting them into split/merge algorithms. Preserve trust-boundary validation and reject invalid transformation requests even if the UI disables them.
3. Preserve original parts and visibility in both exports, add visible results, and ensure score-wide directions and playback tempo survive conversion and transformation. Retaining merged source parts is an intentional behavior change.
4. Extend backend summaries, frontend models, selection initialization, per-tool selection memory, source lists, and warning reporting together. Reuse existing checkbox and status components.

Expected evidence paths: `crates/practice-tracks/src/archive.rs`, `conversion.rs`; `crates/scorewing-core/src/model.rs`, split/merge modules; `apps/desktop/src-tauri/src/lib.rs`; frontend `model.ts`, `app.tsx`, `split-workspace.tsx`, `merge-workspace.tsx`, and their relevant tests.

## Acceptance checks

- Use MuseScore fixtures containing visible and hidden single-staff instruments, an unsupported multi-staff instrument, tempo text and numeric/playback changes, and rehearsal marks on hidden or unchecked carriers. Cover compressed and uncompressed MuseScore inputs.
- Verify every source appears, eligible visible defaults are checked, hidden defaults are unchecked, multi-staff rows are disabled, and all/select/clear cannot enable unsupported or grouped rows.
- Verify switching tools restores each tool's selections, while opening another score resets them. Preserve preparation caching, immutable snapshot handling, failure recovery, and stale-request protection.
- Verify both exports retain all originals with their visibility and include visible generated results from selected hidden and visible sources. Confirm multi-staff content passes through intact and cannot be transformed through direct backend calls.
- Compare mark positions, text, metronome data, and playback changes before and after conversion and export. Verify missing marks produce actionable warnings without blocking export or presenting false success claims.
- Run focused Rust and frontend regression checks, then the repository's applicable checks. Verify warning accessibility, busy states, compact scrolling, and dark appearance. Distinguish mocked conversion tests from proof using the supported MuseScore executable.

## Investigation and current verification

Initial tracing found that `convert_musicxml` called `visible_score`, removing hidden parts and their staff content before MuseScore export. Arrangement summaries lacked visibility and eligibility metadata; multi-staff parts rejected the whole score; selection started empty and reset on tool changes. The implementation changes these ownership points rather than adding a separate import path.

The design handoff used Figcraft to add and visually inspect the three editable frames and export their PNGs. Assert-based gallery checks passed for all three state selectors, accessible control targets, image references, and image dimensions. T3 browser preview could not run because the host lacks required browser libraries; the inline review uses the inspected Figcraft exports.

Implementation coverage includes frontend defaults, tool memory, disabled selection, grouped sources, and persistent warnings; backend metadata and warning caching; original XML and visibility retention; unsupported-part passthrough and request rejection; score-wide directions and exact positions; and direct native conversion followed by split/merge and MusicXML schema validation. The native fixture was exercised using installed MuseScore 4.7 on Linux with an isolated profile.

Final checks passed: 209 Rust workspace tests, workspace formatting, strict workspace Clippy, 54 frontend tests, frontend type checking and production build, and nine focused conversion tests including the native MuseScore case. Repository tooling tests passed 28 checks with the PowerShell-only check skipped. Five opt-in Rust tests were skipped by the normal workspace run; the conversion test was then exercised separately. The four native MP3 audio checks and interactive GUI/platform checks were not run for this change.

Native timing constructs that cannot be verified, including nested tuplets and absolute ticks, produce actionable preservation warnings. Gradual tempo ramp display and playback are explicitly reported as unverified rather than synthesized. These warnings do not block export. Native Windows/macOS behavior and interactive GUI appearance still require platform verification.
