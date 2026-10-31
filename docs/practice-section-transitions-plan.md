# Practice section transitions: implementation plan

Status: implemented using subagents and verified, including inherited-tempo repair at playback jumps. See `docs/VALIDATION.md` for native coverage and limitations.

## Agreed behavior

- Begin a selected section at the score's first normal playback arrival at its rehearsal mark, preserving the repeat/ending/jump state at that arrival.
- A section's written interval still starts at its rehearsal mark and ends immediately before the next mark, or at the last written measure.
- Follow the full written playback route: repeat barlines, numbered endings, D.C., D.S., Coda and Fine, as interpreted by MuseScore.
- Finish repeats whose playback stays within that written interval.
- At the first playback move outside the interval, include exactly two more played measures, then stop. Continue following navigation during those two measures.
- If playback ends sooner, stop there without padding. Count measures in playback order, including partial measures as individual score measures.
- Produce one track per selected section and existing mix/preset selection. Do not add later-pass variants or a configurable overlap.

Examples:

| Score/playback situation | Selected section export |
| --- | --- |
| Next mark begins at measure 20 | Selected section, measure 20, measure 21; stop |
| B ends with a repeat back to A | B, then the first two played measures of A; stop |
| Repeat remains entirely inside B | Finish that repeat, then include two measures after eventually leaving B |
| Only one measure remains after the transition | Include that measure and stop at playback end |
| Playback reaches Fine before leaving the section | Stop at Fine |

## Existing behavior and ownership

`crates/practice-tracks/src/archive/sections.rs` discovers written section intervals, removes navigation, validates extracted measure counts, and repairs boundary ties and inherited tempo. `crates/practice-tracks/src/service.rs` currently strips navigation before native region extraction and reuses each extracted section for its mixes. This prevents real transitions from being rendered.

Keep `PracticeSection` in `crates/practice-tracks/src/lib.rs` and its frontend mirror as inclusive written coordinates. They identify the selected section, not a contiguous description of all music in its output. Expanded playback coordinates belong in private export preparation data.

Reuse the existing MuseScore conversion, region extraction, mix generation, scheduling, cancellation, temporary-file cleanup and publication paths. Keep whole-score export behavior and source files unchanged.

## Implementation sequence

### 1. Prove native playback mapping before choosing the final preparation code

Use the available MuseScore 4.7.0 renderer to export `.mpos` playback positions and a full score with `--unroll-repeats`, from the same normalized disposable score. Upstream 4.7.0 registers `.mpos`; its measure events reference written-measure IDs in native playback order. Native unrolling consumes the same repeat list.

Prove that the event sequence maps one-to-one to the expanded score's measures, with correct written IDs and first-arrival semantics. Cover ordinary/internal repeats, B-to-A transitions, alternate endings, D.C./D.S./Coda/Fine, pickups, inherited tempo and multimeasure rests. Disable collapsed multimeasure rests on the preparation copy if required, and verify the resulting numbering rather than assuming alignment.

This is a correctness gate, not permission to ship an approximate mapping. If `.mpos` cannot reliably provide it, investigate the smallest native-compatible mapping alternative before proceeding. Do not build a separate repeat interpreter or silently fall back to straight playback.

### 2. Prepare playback once and derive private section ranges

Extend the responsible archive/native preparation helpers, adding only private data needed for written-to-played mapping.

1. For exports with selected sections, preserve navigation while preparing the full normalized score.
2. Obtain the validated played-measure sequence and expanded score once per export request; reuse them across selected sections and mixes.
3. For each selected written interval `[start, end]`, locate its first played occurrence of `start`.
4. Starting there, find the first played measure whose written ID lies outside `[start, end]`. Internal repeats stay inside this search.
5. Crop through that outside measure plus the following played measure, clamped to actual playback end. If no exit occurs, crop through playback end.
6. Validate ID bounds, event/expanded measure correspondence, nonempty ranges and reachability of the requested start. Report a clear error if a section is unreachable or mapping is invalid.

Strip residual navigation only after expansion, so extracted audio cannot execute instructions a second time. MuseScore's single-segment unroll path may retain those instructions.

### 3. Integrate with section extraction and existing output lifecycle

Replace the straight-score preparation in `crates/practice-tracks/src/service.rs` with the validated expanded preparation. Pass private played coordinates to existing native `--region` extraction and expected-measure-count validation.

Repair boundary ties and inherited tempo using the expanded source and selected played start. Native proof exposed an inherited-tempo gap at repeat/jump destinations: restore the written destination's inherited numeric tempo at each playback discontinuity, including the default 120 BPM before any tempo mark. Native expansion already restores inherited key and meter. Preserve internal ties and tempo changes, including within the two-measure tail.

Retain existing artifact naming, output counts, extraction reuse, error reporting, partial-result handling and cancellation behavior. Exercise native commands through existing process/error helpers.

### 4. Update the existing UI explanation and documentation

In `apps/desktop/src/practice-view.tsx`, retain the written range display and replace the promise of straight playback without repeats/jumps. Explain that exports follow the score's playback route and include up to two measures after leaving the section.

Reuse existing HeroUI components. Follow the HeroUI skill and MCP documentation when implementing this UI change. No new controls or public metadata fields are planned. Verify the Tauri inspection callers and frontend selection/output counting remain compatible.

Update `README.md`, `docs/REHEARSAL-SECTIONS.md` and `docs/VALIDATION.md` to describe the agreed first-arrival, transition and playback-end rules.

## Subagent execution plan for the later implementation

1. **Native proof/backend worker:** owns native mapping proof, `crates/practice-tracks/src/archive/sections.rs`, any directly responsible private archive/conversion helpers, `crates/practice-tracks/src/service.rs`, and backend unit tests in those owned files. Complete the native proof gate before finalizing implementation.
2. **Regression-test worker:** after the preparation contract is settled, owns `crates/practice-tracks/src/service/parallel_tests.rs` and `crates/practice-tracks/tests/native.rs`, including fixture changes needed there. Use independently expected playback sequences rather than deriving expected results from the new range helper.
3. **UI/documentation worker:** owns `apps/desktop/src/practice-view.tsx`, its relevant tests, `README.md`, `docs/REHEARSAL-SECTIONS.md` and `docs/VALIDATION.md`. Can proceed once the agreed behavior and metadata contract are supplied.

Tell each worker that the workspace is shared, specify exact ownership, and require them to preserve other agents' edits. The parent integrates, resolves small issues and runs focused verification. Avoid overlapping file ownership and unnecessary new abstractions/dependencies.

## Acceptance and focused verification

- Unit tests prove first-arrival selection, exact two-measure cutoff, internal repeats, backward/forward exits, navigation during the tail, unreachable starts and end/Fine clamping. Keep written section-discovery expectations intact.
- Mapping tests prove expanded measure counts and written IDs agree; malformed or mismatched native output fails clearly.
- Native tests compare against independently constructed expected sequences/audio, replacing current assertions that section exports suppress repeats and jumps. Cover alternate endings, D.C./D.S./Coda/Fine, tempo inheritance, pickups, multimeasure rests and boundary ties with small fixtures.
- Adapt the fake native renderer for the extra preparation outputs and private played ranges. Preserve extraction-once reuse, cancellation and partial-result tests.
- Frontend tests cover the revised explanation and unchanged selection/range behavior.
- Run `cargo test -p practice-tracks --locked` and `npm test` / `npm run check` in `apps/desktop`. Run isolated-profile native tests using the setup and command in `docs/VALIDATION.md`; do not treat skipped native tests as playback proof.
- Review the diff for unintended whole-score export, public API, output-count, or source-file changes. Stop after the agreed behavior and these checks pass.

## Evidence and remaining risk

Read-only investigation traced section discovery through service preparation, native extraction, repair, frontend display and existing tests. Codebase-memory used Tier 2 verification with path coverage checks; excluded documentation/test sources were read directly.

Available locally during planning: `/tmp/mxml-native-renderer` reports MuseScore4 4.7.0; its SoundFont is `/tmp/squashfs-root/share/mscore4portable-4.7/sound/MS Basic.sf3`; FFmpeg is `/tmp/ffmpeg-7.0.2-amd64-static/ffmpeg`.

Native capabilities were checked against MuseScore v4.7.0 upstream `notationmodule.cpp`, `positionswriter.cpp`, `unrollrepeats.cpp` and `repeatlist.cpp`. During implementation, native `.mpos` IDs matched expanded unique-note sequences for repeats, alternate endings, D.C./Fine, D.S./Coda and pickups. Multimeasure rests require disabling their layout and removing cached synthetic rest bars on the disposable playback copy; the original score remains unchanged. Automated route/audio acceptance checks and focused validation are recorded in `docs/VALIDATION.md`.

Final checks passed: 45 practice-track unit/service tests, all three native section tests together, the existing general native audio test, frontend tests/type checking, formatting and package-scoped strict Clippy without dependency linting. Native checks ran on Linux with MuseScore 4.7.0. Local Rust checks used the libxml crate's supported shared-library override; release linkage and Windows/macOS rendering were not verified. Dependency-inclusive strict Clippy encountered existing warnings in unchanged `scorewing-core` files.

Suggested implementation commit message: `fix(practice): follow section transitions with two-measure tails`
