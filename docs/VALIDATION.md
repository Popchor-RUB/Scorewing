# Verification

The implementation was verified locally with Python 3.12.15 and lxml 6.0.2 on
Linux. The focused suite has 69 passing tests covering the CLI, exact timing,
split and merge behavior, MusicXML schema validation, identifiers and references,
archive safety, and output protection. `python -m pip check` also passes.

## Supplied scores

Both supplied archives load successfully and their original score XML validates
against the bundled official MusicXML 4.0 schema. The supplied files are not
committed to this repository.

| Input and operation | Result |
| --- | --- |
| `Fame_Druckversion_V1.2.mxl`, split `--mode voices` | Success: 2 original staves retained, 6 derived staves inserted; 125 measures per part. |
| `Empire State of Mind Pt. 2.mxl`, merge `--pair 5+6` | Success: Tenor and Bass merged, 4 unselected staves retained; 58 measures per part. |
| Empire, default chord split | Stops at combined Sopran/Alt, measure 34, position 0: ambiguous tie assignment. |
| Empire, split `--mode voices` | Stops at Alt, measure 7, position 0: ambiguous tie assignment. |
| Fame, default chord split | Stops at combined Sopran/Alt, measure 37, position 0: ambiguous tie assignment. |
| Empire, merge `--pair 2+3` | Stops at Alt, measure 5, position 2: incompatible overlapping rhythmic spans. |

The tie diagnostics were checked against the source events. They occur when a
shared note duplicates into a different set of derived lanes at one tie endpoint.
The editor cannot preserve that relationship under the requested fixed lane
assignment without guessing. The overlap diagnostic identifies unequal rhythmic
spans that cannot form one output voice. These operations produce no output.

Successful output files are saved locally, excluded from version control:

- `dist/examples/Fame-voices.mxl`
- `dist/examples/Empire-tenor-bass.mxl`

The Fame split was checked independently at 3,914 musical intervals: each
derived voice has the expected pitches and exact timing, and original parts
remain unchanged. The Empire merge passed 746 equivalent checks for both input
staves, allowing collapsed unisons. Both generated documents validate against
the official schema and can be reloaded from their compressed archives.

## Rendering

MuseScore 3.6.2 imported and exported representative original, split, and merged
scores. Their first-page PNGs were visually inspected:

- Split: original staves retained, numbered staves directly below their source,
  shared opening notes copied, and later divisi separated.
- Merge: two resulting staves, shared notes collapsed, later voices with up/down
  stems, distinct lyric lines, and bass clef for the lower pair.

No visual defects were found in that representative score. Preview XML and PNGs
are local files under `dist/preview-*`, excluded from version control. This was
not a page-by-page engraving review of the supplied full scores.

## Distribution

A native Linux x64 PyInstaller executable was built and smoke-tested for split,
merge, voice/stem assignment, `.mxl` roundtrips, and bundled offline schema
validation. Python wheel and source distributions also build successfully.

`.github/workflows/build.yml` provides the same tests, executable build, smoke
checks, and checksummed artifacts for Windows x64, Linux x64, macOS Intel, and
macOS Apple Silicon. Windows and macOS jobs have not run in this local session;
they must pass on their native CI runners before those executables are released.
Executables are unsigned. Nothing has been published.

Installed development tools and current versus restart-dependent hook activation
are documented separately in [SETUP.md](SETUP.md).
