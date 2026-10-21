# MXML

A Rust CLI and Tauri 2 desktop app for splitting and merging choir staves in MusicXML.

Open or drop `.mxml`, `.musicxml`, `.xml`, or compressed `.mxl` scores. Select a queued file to see its instruments, then choose **Merge** or **Split** and export a new score. Files are independent; merging across files is not supported.

- **Merge:** create disjoint groups of at least two instruments and order their voices.
- **Split:** select instruments and separate chord tones or explicit voices. Originals remain, with derived staves inserted immediately below them.
- Export one operation at a time. Open an exported file to continue arranging it.

Only score-partwise documents with single-staff parts are supported. Multi-staff instruments such as piano and score-timewise documents are rejected with an explanation. There is no notation rendering or playback.

## Development

Use the Podman devcontainer; no host Rust, Node, or native-library upgrade is needed:

```sh
tools/devcontainer.sh build
tools/devcontainer.sh run cargo test --workspace --locked
tools/devcontainer.sh run bash -c 'cd apps/desktop && npm ci && npm test && npm run build'
```

See [setup](docs/SETUP.md) for VS Code, GUI sessions, build artifacts, and container troubleshooting.

## CLI

```sh
mxml split input.mxl -o split.musicxml
mxml split input.musicxml -o voices.mxl --mode voices --part Soprano --part 3
mxml merge input.mxl -o merged.mxl --pair 'Soprano+Alto' --pair '3+4'
```

Selectors are one-based positions or exact part names. Use positions when names repeat. Merge groups cannot overlap; the order inside each group determines voice order. Unselected parts remain unchanged. Omitting `--part` splits every eligible part, preserving the previous CLI default.

Split scans the entire score, retains original staves, and adds numbered derived staves only when needed. Chord mode separates tones; voices mode keeps chords within explicit voices. A voice absent from a measure follows voice 1; explicit rests remain rests. At shared-voice transitions, split repairs tie endpoints and uses a slur when connected single notes change pitch. Other ambiguous ties/slurs and incompatible merge rhythms cause an error rather than guessed musical content.

## Files and safety

Source replacement is refused, including aliases and hard links. GUI exports protect all queued sources. Existing outputs require confirmation in the GUI or `--force` in the CLI.

Generated scores must pass structural, identifier/reference, and bundled official MusicXML 4.0 schema checks before atomic publication. Failed operations leave existing files intact. XML entities, unsafe archive paths, symbolic archive members, and inputs above 64 MiB are rejected; no external DTD or network lookup is required.

Linux packages require Ubuntu 24.04-compatible GTK/WebKitGTK and libxml2. CI also defines unsigned Windows and macOS builds. See [validation](docs/VALIDATION.md) for checks and platform limits.
