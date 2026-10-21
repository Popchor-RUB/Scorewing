# mxml

A cross-platform CLI for splitting and merging choir staves in MusicXML.
Supports `.mxml`, `.musicxml`, `.xml`, and compressed `.mxl` score-partwise files.
Each single-staff part is one stave. Multi-staff instruments and score-timewise
documents are rejected with an explanation.

## Install

Use a standalone executable from the native build artifacts, or install with
Python 3.11 or newer:

```sh
python -m pip install .
mxml --help
```

The standalone executable includes Python, lxml, and offline MusicXML schemas.
Build instructions and tooling activation status are in [docs/SETUP.md](docs/SETUP.md).

## Split

```sh
mxml split choir.mxl -o split.mxl
mxml split choir.musicxml -o split.musicxml --mode voices
```

Every original stave is retained. Derived staves appear immediately below it,
named `Soprano 1`, `Soprano 2`, and so on. Entirely monophonic staves remain alone.

The default `chords` mode separates explicit voices, then orders chord tones
from highest to lowest. `voices` mode separates explicit voices while keeping
their chords. Voice slots are stable across the entire part, ordered by numeric
voice ID, followed by nonnumeric IDs in lexical order. No melodic crossing is
inferred.

Single notes are shared among that voice's chord-derived staves. Passages with
only one encoded voice are shared with absent voices. Explicit rests remain
rests; vacant slots in partially populated chords receive rests. Every derived
stave uses voice 1 and retains complete measure timing, including pickups.
Assignments that would break ties or slurs are rejected with a part, measure,
and position rather than silently changing the music.

## Merge

```sh
mxml merge choir.mxl -o merged.mxl --pair 1+2 --pair 3+4
mxml merge choir.mxl -o merged.mxl --pair "Soprano+Alto"
mxml merge choir.mxl -o merged.mxl --pair 1+2+3
```

At least one `--pair` is required. Each group accepts two or more distinct staves,
using one-based input positions, exact part names, or a mixture. Groups cannot
overlap. Argument order assigns voices 1, 2, 3, etc.; it does not change selectors
in later groups. A merged stave replaces its members at their earliest original
position, with names joined by ` / `. Unselected staves remain intact.

Odd voices have up-stems; even voices have down-stems. Whole notes and other
stemless values remain stemless. Each measure starts in shared voice 1 when all
members match in spelled pitch, onset, duration, and tie state. Matching rests
also collapse. At the first difference, assigned voices remain separate until
the barline, even if the notes reunite. Sustained notes cut at a divergence are
tied across the cut.

Existing voices within a selected stave are flattened. Simultaneous notes with
equal rhythmic spans become chords. Unequal overlapping spans cannot become
one output voice and are rejected. Distinct lyrics receive stable separate
lines; identical lyrics are deduplicated. Source-specific directions retain
their voice. The merged stave uses the treble or bass clef with fewer ledger
lines across the part, choosing treble on a tie. Encoded pitches are preserved.
Measure boundaries, time and key signatures, and transpositions must agree.

## Files and safety

`-o` is required; its extension selects plain or compressed output. The input
cannot be replaced, even with `--force`. Existing output requires `--force`.
Output is checked against the bundled official MusicXML 4.0 schema and written
atomically only after successful validation. Errors go to stderr with exit code
2; success returns 0.

External XML resources are disabled. Entity declarations, unsafe archive paths,
duplicate archive members, and archives with symbolic links are rejected. XML
inputs and decompressed XML members are limited to 64 MiB.

The editor retains score metadata, part groups, lyrics, note relationships,
tuplets, annotations, directions, and repeats in the XML. Affected notes lose
stale drawing coordinates so engraving software can position them again.
The output is a new engraving; page breaks and spacing may change.

## Development

```sh
python -m pip install -e . -r requirements-build.txt
python -m pytest -q
```

See [docs/VALIDATION.md](docs/VALIDATION.md) for verification and real-score results.
Native CI builds unsigned Windows x64, Linux x64, macOS Intel, and macOS Apple
Silicon executables with SHA-256 checksums. Publishing and signing are separate.
