# Rehearsal section practice tracks

Status: implemented; requirements and acceptance checks are recorded below.

## Purpose

Export practice tracks for selected rehearsal sections in addition to the existing
whole-song tracks. Identify sections from genuine MuseScore rehearsal marks.

## Agreed behavior

- Support the existing `.mscx` and `.mscz` score inputs.
- Offer a list of detected sections with independent selection. Whole-song export
  is also selectable and is enabled by default. Users may export sections only.
- A section's displayed written range starts at the beginning of the measure
  containing its rehearsal mark and ends immediately before the next marked
  measure. A mid-measure mark snaps to that measure's start; it does not create
  a beat-level boundary. The range identifies the section, not every measure
  played in its track.
- If there are measures before the first marked measure, offer an `Intro` section
  covering them. Do not create an empty Intro when the first measure is marked.
- The last marked section continues through the end of the score.
- Multiple marks in the same measure create one section with combined names,
  such as `B + Trio`. Repeated labels in different measures remain distinct
  sections, identified by score position and export order.
- Use the existing instrument selection, foreground/background sounds, background
  volume, and metronome settings for every selected section and the whole song.
  Do not add per-section mix settings.
- Section playback begins at the first normal playback arrival at its marked
  measure, preserving MuseScore's repeat/ending/jump state at that arrival.
  It starts immediately, with no count-in or preceding measure. Follow repeats,
  numbered endings, D.C., D.S., Coda and Fine as MuseScore plays them.
- Finish repeats whose playback stays within the section's written range. At
  the first playback move outside that range, include exactly two more played
  measures, following navigation during those measures too. Stop sooner if
  playback ends or reaches Fine; do not pad the track. Count pickups and other
  partial measures as individual played measures.
- For example, a next mark at measure 20 includes measures 20 and 21 before
  stopping. If B instead repeats back to A, B's track includes the first two
  played measures of A. Produce one track per section and selected target,
  without later-pass variants or an overlap control.
- Preserve the existing repeat/jump behavior for whole-song tracks. Allow final
  notes and reverb to decay naturally instead of applying a hard cutoff or fade.
- Place each selected range in its own ordered folder under the existing export
  directory. Use `00 - Whole song` when included, then section folders in score
  order, for example `01 - Intro`, `02 - A`, and `03 - B`. Keep the existing
  instrument-track filenames inside each folder.
- Preserve mark text in displayed names; sanitize filesystem names and retain
  ordering prefixes so duplicate labels cannot overwrite one another.
- A score without rehearsal marks remains eligible for whole-song export and
  offers no rehearsal sections. Prevent an export with no selected range.

Only actual rehearsal-mark objects define sections. Ordinary staff/system text
does not become a section merely because its appearance includes a box.

## Confirmed technical facts

MuseScore represents rehearsal marks in MSCX XML as a `RehearsalMark` element
containing its label, for example `<RehearsalMark><text>A</text></RehearsalMark>`.
MSCZ contains the master MSCX document. Mark text can contain rich formatting,
and marks can occur within a measure.

The existing application already reads these score containers in
[`archive.rs`](../crates/practice-tracks/src/archive.rs), normalizes scores and
renders instrument mixes in
[`service.rs`](../crates/practice-tracks/src/service.rs), and requires MuseScore
Studio 4.7.x in [`renderer.rs`](../crates/practice-tracks/src/renderer.rs).
Reuse those responsibilities and the existing mix request types in
[`lib.rs`](../crates/practice-tracks/src/lib.rs).

MuseScore 4.7 provides a native region-to-MSCZ export option. Its JSON accepts
one-based measure and staff numbers, for example:

```json
{"start":{"measure":3,"staff":1},"end":{"measure":8,"staff":4}}
```

The option is `--region '<JSON>'` with an output score supplied through `-o`.
The implementation uses this capability to extract all-staff section scores once
per selected range from an expanded playback score before using the existing
mixing/rendering pipeline. Native checks confirmed that the start and end
measure numbers are both inclusive.

Authoritative MuseScore 4.7 sources:

- [Rehearsal-mark fixture](https://github.com/musescore/MuseScore/blob/v4.7.0/src/engraving/tests/compat206_data/textstyles-ref.mscx#L395)
- [Rehearsal-mark reader](https://github.com/musescore/MuseScore/blob/v4.7.0/src/engraving/rw/read460/measureread.cpp#L499)
- [CLI region option](https://github.com/musescore/MuseScore/blob/v4.7.0/src/app/internal/commandlineparser.cpp#L135)
- [Region argument parser](https://github.com/musescore/MuseScore/blob/v4.7.0/src/converter/internal/converterutils.cpp#L118)
- [Native selection export](https://github.com/musescore/MuseScore/blob/v4.7.0/src/converter/internal/convertercontroller.cpp#L591)

## Implementation and verification

Native MuseScore 4.7.0 checks previously verified inclusive region boundaries,
selection across staves, inherited key/meter and part identity. Section playback
regressions compare against independently expected playback sequences and
standalone scores, including inherited tempo, immediate playback, pickups,
navigation, boundary ties and final-note decay. See [validation](VALIDATION.md)
for the native test setup and coverage. No fixed-duration bar calculations or
audio cropping are used.

Native region export drops tempo annotations from before the section and retains
tie references to notes outside it. The archive layer restores the preceding
tempo at the beginning of the extracted score and removes only ties crossing its
boundaries. Ties within the section remain intact.

Native expansion restores inherited key and meter at repeat/jump destinations,
but can leave the outgoing tempo active when the destination has no tempo mark.
The archive layer restores the written destination's inherited tempo at each
playback jump, including the default 120 BPM before any explicit tempo mark.
This repair also applies to jumps within a section and within its transition tail.

Section preparation uses MuseScore's playback positions and `--unroll-repeats`
output to map written measures to playback order. It locates the selected
section's first arrival and crops the expanded score through the two-measure
transition tail, clamped to playback end. Invalid mappings and unreachable
section starts fail with an explanation. Residual navigation is removed after
expansion so the cropped score cannot execute it again. Source scores and
whole-song playback remain unchanged.

Each selected range is extracted once and reused for the chosen instrument mixes.
The existing scheduler, progress, cancellation, and publication behavior account
for every selected range and mix. Regression tests cover collision-safe output
creation, completed files on failure/cancellation across ranges, and renderer
cleanup. HeroUI checkboxes expose ranges and keep choices local to the document.
The backend validates section IDs against its immutable score snapshot.

## Acceptance checks

1. Parse rehearsal labels and measure positions from both input containers,
   including formatted labels. Exclude ordinary boxed text.
2. Verify no-mark scores, a first-measure mark, an Intro, the final section,
   mid-measure snapping, multiple marks in one measure, and duplicate or unsafe
   label text. Every offered section contains at least one measure.
3. Export only selected sections, optionally alongside the whole song, using the
   shared mix settings and ordered section directories. Reject empty selection.
4. Verify first-arrival selection, internal repeats, forward and backward exits,
   and navigation during the exact two-measure tail. Check alternate endings,
   D.C./D.S./Coda/Fine and early playback end, including unreachable starts and
   invalid native mappings. Whole-song repeats/jumps retain existing behavior.
   Use native rendering checks for inherited musical state, pickups,
   multimeasure rests, boundary notes and natural decay.
5. Verify progress and cancellation across multiple ranges and mixes, plus
   completed-output preservation if a later render fails.

Native audio checks were performed on Linux. Windows/macOS native rendering
still needs verification on those platforms.
