# Voiceweaver

A Rust CLI and Tauri 2 desktop app for splitting and merging choir staves in MusicXML, with desktop practice-track creation for MuseScore scores.

Builds produce the `voiceweaver` CLI and `voiceweaver-desktop` executable, with desktop installers branded **Voiceweaver**.

Open or drop `.mxml`, `.musicxml`, `.xml`, or compressed `.mxl` scores. Select a queued file to see its instruments, then choose **Merge** or **Split** and export a new score. Files are independent; merging across files is not supported.

- **Merge:** create disjoint groups of at least two instruments, order their voices, and choose each voice’s up/down stem direction. Matching notes with the same stem direction, onset, duration, and tie state collapse into the earliest voice, combining lyrics and markings.
- **Split:** select instruments and separate chord tones or explicit voices. Originals remain, with derived staves inserted immediately below them. Each selected instrument previews its additional staves and offers a fallback voice for each one.
- **Create Practice Tracks:** open a MuseScore 4 `.mscz` or `.mscx` score, choose foreground/background MS Basic sounds for each instrument, and independently select background members and practice-track targets. Hidden parts remain selectable and multi-staff instruments remain one target.
- Export one operation at a time. Open an exported file to continue arranging it.

MusicXML arranging supports score-partwise documents with single-staff parts. Multi-staff MusicXML instruments and score-timewise documents are rejected with an explanation. The app does not display notation or provide an audio player.

In the desktop app, each split staff uses its written voice whenever that voice appears anywhere in the measure, preserving written rests and gaps. In chord mode, a compatible voice allocated to an extra chord-tone staff takes priority in its own measures. Only when all native sources are absent for the whole measure does the staff use its selected fallback; **None**, or an absent fallback voice, produces silence. Fallbacks never chain. In chord mode, the same chord-tone selection applies to the fallback voice.

Ties continue automatically across fallback voice transitions when pitches match. **Automatically insert slurs at fallback voice transitions** is enabled by default; turn it off to replace the fallback note with a rest of the same duration when a tie would connect different pitches. Existing slurs are preserved. Generated slurs and their endpoint notes are marked red on the affected derived staff, with measure, voice, and pitch details shown in the export warnings.

Fallback suggestions match consistent explicit up/down stems and prefer the matching voice present in the most measures where the target voice is absent. Missing or conflicting stems and tied candidates default to **None**. Suggestions are editable, and choices are kept separately for each queued file and split mode until the file is closed.

## Practice tracks

Install **MuseScore Studio 4.7.x** separately. Voiceweaver discovers its executable and bundled **MS Basic.sf3**; renderer settings also accept explicit paths. For a Linux AppImage, extract it with `--appimage-extract` and select the executable and matching SoundFont from that installation. Python, Muse Sounds, and online sound engines are unnecessary.

Each selected target produces a 192 kbps MP3. The target uses its foreground sound at 0 dB; other selected background members use their background sounds, reduced by 6 dB by default (adjustable from 0–60 dB). Parts outside those roles are silent. Note velocity is fixed at 80 and expressive volume changes are disabled; notation, pitches, tempo, and repeats are retained. Mixer gain is not perceived loudness, and large ensembles can clip; no normalization is applied.

The metronome checkbox controls **MuseScore's built-in metronome**, with an independent level from −60 to 0 dB (default −6 dB). No percussion staff or generated click notes are added. Keep MuseScore closed during exports: Voiceweaver temporarily changes only its metronome preference, restores its original value or absence afterward, and keeps a recovery journal if interrupted. Unresolved preference conflicts or unsupported preference layouts stop rendering with an explanation. Concurrent Voiceweaver jobs share a preference-store lock.

Choose a destination folder to create a fresh numbered results folder. Existing files and source scores are preserved. Cancellation or a failed conversion retains completed MP3s and reports their paths. Settings remain with each queued score until it is closed. Practice tracks are currently a desktop feature; the Rust CLI remains focused on MusicXML arranging.

Linux audio acceptance has been verified with MuseScore 4.7.5 and its bundled MS Basic SoundFont, including native metronome timing, gain and preference restoration. Native Windows/macOS rendering and preference restoration still require verification on those hosts; unit tests alone do not establish platform compatibility.

## Development

Use the Podman devcontainer; no host Rust, Node, or native-library upgrade is needed:

```sh
tools/devcontainer.sh build
tools/devcontainer.sh run cargo test --workspace --locked
tools/devcontainer.sh run bash -c 'cd apps/desktop && npm ci && npm test && npm run build'
```

See [setup](docs/SETUP.md) for VS Code, GUI sessions, build artifacts, and container troubleshooting.

For local release builds, run `tools/build-linux.sh`, `tools/build-macos.sh`, or `powershell -File tools/build-windows.ps1`. These scripts report missing prerequisites and never install dependencies. Linux prefers the prepared devcontainer; macOS and Windows build natively. The macOS script ad-hoc signs the app and installs it in `/Applications`. See [local compilation prerequisites](docs/SETUP.md#local-compilation-without-installing-dependencies).

## CLI

```sh
voiceweaver split input.mxl -o split.musicxml
voiceweaver split input.musicxml -o voices.mxl --mode voices --part Soprano --part 3
voiceweaver merge input.mxl -o merged.mxl --pair 'Soprano+Alto' --pair '3+4'
```

Selectors are one-based positions or exact part names. Use positions when names repeat. Merge groups cannot overlap; the order inside each group determines voice order. Unselected parts remain unchanged. Omitting `--part` splits every eligible part, preserving the previous CLI default.

Split scans the entire score, retains original staves, and adds numbered derived staves only when needed. Chord mode separates tones and reuses an extra chord-tone staff for a monophonic voice with matching staff and stem direction when their required measures never overlap. Actual source notes and rests take priority over unison doubling and fallbacks; ambiguous allocations remain separate. Ties and slurs involving real chord tones reserve their connected measures. Voices mode keeps chords within explicit voices. The CLI retains its legacy sharing behavior: a voice absent from a measure follows voice 1, and a lone active voice fills gaps; explicit rests remain rests. At shared-voice transitions, split repairs tie endpoints and uses a slur when connected single notes change pitch. Incompatible merge rhythms cause an error rather than guessed musical content.

## Files and safety

Ambiguous tie or slur assignments produce review warnings and still export a best-effort split. Assigned notes and timing are preserved; uncertain or incomplete connections are omitted from derived staves. Affected passages are colored red on the retained original and corresponding split staves in the exported score. The input file is unchanged. The desktop app lists warnings alongside the export path; the CLI prints warnings to stderr and exits successfully after writing. The core split functions return `SplitResult { root, warnings }`.

Source replacement is refused, including aliases and hard links. GUI exports protect all queued sources. Existing outputs require confirmation in the GUI or `--force` in the CLI.

Generated scores must pass structural, identifier/reference, and bundled official MusicXML 4.0 schema checks before atomic publication. Failed operations leave existing files intact. XML entities, unsafe archive paths, symbolic archive members, and inputs above 64 MiB are rejected; no external DTD or network lookup is required.

The CLI and desktop app link libxml2 statically; users do not need Homebrew or a separate libxml2 installation. Platform runtimes remain dynamic: macOS system frameworks, Windows system DLLs/WebView2, and Ubuntu 24.04-compatible GTK/WebKitGTK for Linux desktop packages. CI also defines unsigned Windows and macOS builds. See [validation](docs/VALIDATION.md) for checks and platform limits.
