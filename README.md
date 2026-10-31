# Scorewing

A Rust CLI and Tauri 2 desktop app for splitting and merging choir staves in MusicXML, with desktop practice-track creation for MuseScore scores.

Builds produce the `scorewing` CLI and `scorewing-desktop` executable, with desktop installers branded **Scorewing**.

Existing automation must adopt the renamed commands and `SCOREWING_*` build variables; no legacy aliases are provided. See [rename and upgrade notes](docs/SETUP.md#rename-and-upgrade-notes) for preserved settings and installation behavior. The existing icon artwork is unchanged.

Open or drop one `.mxml`, `.musicxml`, `.xml`, compressed `.mxl`, or MuseScore `.mscz`/`.mscx` score, then choose an operation and export a new score. Select or drop two scores together to compare versions, including a MusicXML/MuseScore pair. A confirmation dialog stacks **Older version** above **Newer version**, with a swap button between them. Click either filename to choose a replacement, then **Export diff** to export directly without opening the diff workspace. With one score loaded, choose **Diff versions** to use the same controls below the mode selector, with the loaded score initially assigned as Newer. Successful diff exports return to the start screen; cancelling the save dialog or an export failure keeps the selections for retry. Use **Close score** to return to the start, or **Open another score** to replace it. Closing or replacing a score discards its editing choices; cancelling the picker or failing to load a replacement keeps the current score.

- **Merge:** create disjoint groups of at least two instruments, order their voices, and choose each voice’s up/down stem direction. Matching notes with the same stem direction, onset, duration, and tie state collapse into the earliest voice, combining lyrics and markings.
- **Split:** select instruments and separate chord tones or explicit voices. Originals remain, with derived staves inserted immediately below them. Each selected instrument previews its additional staves and offers a fallback voice for each one.
- **Diff versions (desktop):** select two scores, or open the newer score and choose an older version, and export the newer score as compressed `.mxl` by default, with changed pitches, durations, voices, lyrics, dynamics, chord symbols, and tie/slur endpoints marked red. Plain `.musicxml` and `.xml` exports are also available; `.mxml` is accepted for compatibility but may not be recognized by notation apps. Initial ordering uses a filename containing `old`, `alt`, or `outdated` (case-insensitively), then the lower semantic version when both names have one unambiguous version, then the earlier modification time. Ambiguous or equal rules fall through; a final tie keeps selection order with the first file as old. Confirm or swap this order before comparing. Insertions and deletions are aligned; removed content marks nearby surviving music and produces review warnings. Sound-equivalent pitches such as C♯/D♭ compare as equal, including instrument transposition. Layout, generated IDs, and relationship renumbering are ignored. Parts match by unique names, then stable IDs; ambiguous part matches stop the export. Repeated music may have ambiguous alignment, reported in warnings. Both input snapshots and their files remain protected; the newer score's content and layout are retained. There is no notation preview or CLI diff command.
- **Create Practice Tracks:** open a MuseScore 4 `.mscz` or `.mscx` score, choose foreground/background MS Basic sounds for each instrument, and independently select background members and practice-track targets. Hidden parts remain selectable and multi-staff instruments remain one target.
- Export one operation at a time. Open an exported file to continue arranging it.

MusicXML arranging supports score-partwise documents with single-staff parts. Multi-staff MusicXML instruments and score-timewise documents are rejected with an explanation. The app does not display notation or provide an audio player.

MuseScore scores open in practice-track mode by default. Merge, split, and diff convert the original input snapshot to MusicXML through the separately installed MuseScore Studio 4.7.x CLI; this conversion needs no SoundFont. Only explicitly visible instruments and staves are included, while empty-staff layout rules do not remove music. Instruments with multiple remaining visible staves retain the arranging engine's multi-staff restriction. Practice tracks continue to use the original score, including hidden parts. Source files are never modified.

In the desktop app, each split staff uses its selected fallback before its written voice first appears in a measure, or throughout the measure if that voice never appears. Written rests and grace notes count as the voice appearing; subsequent gaps remain silent. In chord mode, a compatible voice allocated to an extra chord-tone staff takes priority in its own measures, with fallback used before that voice's first event. **None**, or an absent fallback voice, produces silence. Fallbacks never chain. In chord mode, the same chord-tone selection applies to the fallback voice.

Ties continue automatically across fallback voice transitions when pitches match. **Automatically insert slurs at fallback voice transitions** is enabled by default; turn it off to replace the fallback note with a rest of the same duration when a tie would connect different pitches. Existing slurs are preserved. Generated slurs and their endpoint notes are marked red on the affected derived staff, with measure, voice, and pitch details shown in the export warnings.

Fallback suggestions match consistent explicit up/down stems and prefer the matching voice present in the most measures where the target voice is absent. Missing or conflicting stems and tied candidates default to **None**. Suggestions are editable, and choices are kept separately for each split mode until the score is closed or replaced.

## Practice tracks

Install **MuseScore Studio 4.7.x** separately. Scorewing discovers its executable and bundled **MS Basic.sf3**; renderer settings also accept explicit paths. For a Linux AppImage, extract it with `--appimage-extract` and select the executable and matching SoundFont from that installation. Python, Muse Sounds, and online sound engines are unnecessary.

Each selected target produces a 192 kbps MP3 for every selected range. Whole-song export is selected by default; rehearsal sections can be selected alongside it or exported on their own. Sections follow genuine MuseScore rehearsal marks. Their displayed ranges run from the marked measure through the measure before the next mark. An Intro covers measures before the first mark; the last section reaches the written end. Marks in the same measure share a section and combined name. Ordinary boxed text does not define a section.

Section tracks begin at the first normal playback arrival at their marked measure and follow MuseScore's repeats, numbered endings, D.C., D.S., Coda and Fine. Repeats that stay within the displayed range finish before the transition. At the first move outside that range, the track includes two more played measures, or stops sooner at playback end or Fine. For example, if B repeats back to A, B's track finishes with the first two played measures of A. Measures are counted in playback order, including pickups and other partial measures. Each selected section produces one track per selected target, without later-pass variants.

The target uses its foreground sound at 0 dB; other selected background members use their background sounds, reduced by 6 dB by default (adjustable from 0–60 dB). The same settings apply to all selected ranges. Parts outside those roles are silent. Note velocity is fixed at 80 and expressive volume changes are disabled; notation, pitches, and tempo are retained. Whole-song tracks keep their existing repeats and jumps. Sections start immediately without a count-in and allow the final sound to decay. Mixer gain is not perceived loudness, and large ensembles can clip; no normalization is applied.

Track rendering uses separate MuseScore processes, limited to half the available logical CPUs, rounded down, with at least one worker and no more workers than selected tracks. If CPU detection fails, exports use one worker. Each process loads its own score and sounds, so large exports on machines with many CPUs can use substantial memory. Completed tracks can appear out of order; filenames and the final track list retain the selected order.

The metronome checkbox controls **MuseScore's built-in metronome**, with an independent level from −60 to 0 dB (default −6 dB). No percussion staff or generated click notes are added. Keep MuseScore closed during exports: Scorewing temporarily changes only its metronome preference, restores its original value or absence afterward, and keeps a recovery journal if interrupted. Unresolved preference conflicts or unsupported preference layouts stop rendering with an explanation. Concurrent Scorewing jobs share a preference-store lock.

Foreground and background instrument fields open the full sound catalog on focus. Type to filter, use arrow keys to navigate, and press Enter to select. The five most recently selected sounds appear above the unfiltered catalog, shared across selectors and remembered between app sessions.

Choose a destination folder to create a fresh numbered results folder. Each selected range has an ordered subfolder, such as `00 - Whole song`, `01 - Intro`, and `02 - A`, containing the instrument MP3s. Existing files and source scores are preserved. Cancellation or a failed conversion retains completed MP3s and reports their paths. Score-specific settings remain until the score is closed or replaced; renderer paths and recent instruments survive score changes. Practice tracks are currently a desktop feature; the Rust CLI remains focused on MusicXML arranging.

Linux audio acceptance has been verified with MuseScore 4.7.5 and its bundled MS Basic SoundFont, including native metronome timing, gain and preference restoration. Concurrent native rendering still requires verification. Native Windows/macOS rendering and preference restoration still require verification on those hosts; unit tests alone do not establish platform compatibility.

## Development

Use the Podman devcontainer; no host Rust, Node, or native-library upgrade is needed:

```sh
tools/devcontainer.sh build
tools/devcontainer.sh run cargo test --workspace --locked
tools/devcontainer.sh run bash -c 'cd apps/desktop && npm ci && npm test && npm run build'
```

See [setup](docs/SETUP.md) for VS Code, GUI sessions, build artifacts, and container troubleshooting.

The desktop frontend uses npm, Vite, TypeScript, React 19, Tailwind CSS v4, and HeroUI v3. Start the Vite dev server at `http://localhost:1420` with:

```sh
cd apps/desktop
npm ci
npm run dev
```

Run `npm run tauri dev` instead to open the native desktop app with its Rust backend (requires the platform prerequisites in the setup guide). The editor uses React and HeroUI components throughout; native score operations require Tauri. Styles load through `src/main.ts` → `src/style.css`, which imports Tailwind before HeroUI. UI tests mock native commands and exercise the editor and searchable sound selectors.

For local release builds, run `tools/build-linux.sh`, `tools/build-macos.sh`, or `powershell -File tools/build-windows.ps1`. These scripts report missing prerequisites and never install dependencies. Linux prefers the prepared devcontainer; macOS and Windows build natively. The macOS script ad-hoc signs the app and installs it in `/Applications`. See [local compilation prerequisites](docs/SETUP.md#local-compilation-without-installing-dependencies).

## CLI

```sh
scorewing split input.mxl -o split.musicxml
scorewing split input.musicxml -o voices.mxl --mode voices --part Soprano --part 3
scorewing merge input.mxl -o merged.mxl --pair 'Soprano+Alto' --pair '3+4'
```

Selectors are one-based positions or exact part names. Use positions when names repeat. Merge groups cannot overlap; the order inside each group determines voice order. Unselected parts remain unchanged. Omitting `--part` splits every eligible part, preserving the previous CLI default.

Split scans the entire score, retains original staves, and adds numbered derived staves only when needed. Chord mode separates tones and reuses an extra chord-tone staff for a monophonic voice with matching staff and stem direction when their required measures never overlap. Actual source notes and rests take priority over unison doubling and fallbacks; ambiguous allocations remain separate. Ties and slurs involving real chord tones reserve their connected measures. Voices mode keeps chords within explicit voices. The CLI retains its legacy sharing behavior: a voice absent from a measure follows voice 1, and a lone active voice fills gaps; explicit rests remain rests. At shared-voice transitions, split repairs tie endpoints and uses a slur when connected single notes change pitch. Incompatible merge rhythms cause an error rather than guessed musical content.

## Files and safety

Ambiguous tie or slur assignments produce review warnings and still export a best-effort split. Assigned notes and timing are preserved; uncertain or incomplete connections are omitted from derived staves. Affected passages are colored red on the retained original and corresponding split staves in the exported score. The input file is unchanged. The desktop app lists warnings alongside the export path; the CLI prints warnings to stderr and exits successfully after writing. The core split functions return `SplitResult { root, warnings }`.

Source replacement is refused, including aliases and hard links. GUI exports protect loaded sources. Existing outputs require confirmation in the GUI or `--force` in the CLI.

Generated scores must pass structural, identifier/reference, and bundled official MusicXML 4.0 schema checks before atomic publication. Failed operations leave existing files intact. XML entities, unsafe archive paths, symbolic archive members, and inputs above 64 MiB are rejected; no external DTD or network lookup is required.

The CLI and desktop app link libxml2 statically; users do not need Homebrew or a separate libxml2 installation. Platform runtimes remain dynamic: macOS system frameworks, Windows system DLLs/WebView2, and Ubuntu 24.04-compatible GTK/WebKitGTK for Linux desktop packages. CI also defines unsigned Windows and macOS builds. See [validation](docs/VALIDATION.md) for checks and platform limits.
