# Validation

Rust unit/integration tests cover musical timing, transformations, file safety, CLI behavior, desktop sessions, and practice-track preparation and export. Frontend tests cover arranging and practice-track workflows. The MusicXML engine's former Python test scenarios are covered directly in Rust; its reference implementation and parity driver have been removed. The separately supplied MuseScore practice-track reference is not a runtime dependency.

| Coverage | Rust test location |
| --- | --- |
| Exact timing, chords, slicing, relationship styles, invalid scores | `crates/mxml-core/src/model.rs` unit tests |
| Plain/compressed roundtrips, archive safety, schema validation, atomic output protection | `crates/mxml-core/src/io.rs` unit tests |
| Split ordering, voice/chord sharing, ties/slurs, annotations, metadata and identifiers | `crates/mxml-core/tests/split.rs` |
| Merge ordering, rhythms, rests, lyrics, instruments, compatibility and relationships | `crates/mxml-core/tests/merge.rs` |
| CLI split/merge roundtrips, mixed name/position selectors, invalid groups and file protection | `crates/mxml-cli/tests/cli.rs` |
| MuseScore archives, MS Basic presets, sound roles, membership, normalization, fixed dynamics and native metronome configuration | `crates/practice-tracks/src/archive.rs` and `catalog.rs` unit tests |
| Renderer failures/timeouts, partial results, atomic publication, metronome preference locking/restoration/crash recovery | Practice-track renderer, service and preference unit tests using isolated fake stores |
| Opt-in real MuseScore MP3 gain, sound roles, dynamics, excluded-part silence and built-in metronome audio/restoration | `crates/practice-tracks/tests/native.rs` |

Before removing the reference suite, the expanded Rust workspace tests passed alongside all 73 Python reference cases and 42 Rust-engine parity cases. Equivalent assertions extend existing Rust tests where possible rather than duplicating each Python test function. Ambiguous ties/slurs retain the current Rust warning-and-export behavior. Archive path tests exercise raw member names directly, replacing the Python-specific separator monkeypatch.

```sh
tools/devcontainer.sh run bash tools/check.sh
```

The shared engine embeds the official MusicXML 4.0 schemas, with imports restricted to local bundled files. Output validation requires no Python installation and no network access. Schema licensing and provenance remain alongside the schema files.

CI builds unsigned native artifacts for Linux x64, Windows x64, macOS Intel, and macOS Apple Silicon. Native builds and interactive GUI behavior on platforms unavailable locally must be verified by their respective runners; a configured workflow alone does not establish that those builds passed.

CI caches Cargo downloads and compiled Rust dependencies between runs. Linux also caches workspace build output: `MXML_CACHE_DIR` tells `tools/devcontainer.sh` to mount directories restored by GitHub Actions instead of local named volumes. Native jobs use `Swatinem/rust-cache`. Cache keys separate platforms and build environments, with Linux fallback keys allowing unchanged dependencies to survive lockfile updates. Cargo still decides what must be rebuilt, and all checks and packaging steps run on every execution. The first run or an evicted cache requires a full build; Linux container images and native XML dependencies are still built on each runner.

Release checks use `otool -L` on macOS, `readelf -d` on Linux, and LLVM's PE import inspection on Windows to reject shared application dependencies in both CLI and desktop executables. macOS permits only system framework/library paths (and rejects libxml2 even there); Windows permits system DLLs without a dynamic Visual C++ runtime; Linux permits the C runtime and, for the desktop only, GTK/WebKitGTK integration libraries. Linux platform libraries may themselves depend on the system libxml2; our executables must not import it directly. `node --test tools/check-linkage.test.mjs` covers missing-libxml2 regressions and rejects unrecognized inspection output.

The automated gate runs Rust workspace tests, frontend workflow tests and type checking, formatting, strict workspace Clippy checks, release builds, and linkage inspection. Earlier manual validation launched the release app under Xvfb and its real native Open dialog loaded the existing four-instrument sample. DOM tests additionally simulate native file-drop and export-dialog responses.

Inputs used in previous manual musical inspection are not committed. No page-by-page engraving review, playback check, signing, or notarization is claimed by the automated suite.

## Native practice-track audio checks

The ignored Linux acceptance test requires MuseScore Studio 4.7.x, the matching installed MS Basic SoundFont, and FFmpeg. It measures decoded MP3 audio and verifies the native metronome preference is restored for initially enabled, disabled, and absent values. It passed with the official MuseScore 4.7.5 AppImage and bundled MS Basic, including foreground/background gain, distinct sounds, fixed dynamics, excluded-note/chord-symbol silence, and metronome timing through pickups, tempo/meter changes, and repeats. These checks must run in a disposable profile: the test checks canonical HOME and XDG_CONFIG_HOME paths before writing preferences and refuses preference symlinks.

```sh
practice_profile=$(mktemp -d)
mkdir -p "$practice_profile/home" "$practice_profile/config"
VOICEWEAVER_NATIVE_PROFILE="$practice_profile" \
HOME="$practice_profile/home" XDG_CONFIG_HOME="$practice_profile/config" \
MUSESCORE_EXECUTABLE=/absolute/path/to/MuseScore \
MUSESCORE_SOUNDFONT="/absolute/path/to/MS Basic.sf3" \
FFMPEG_EXECUTABLE="$(command -v ffmpeg)" \
MU_QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORM=offscreen \
cargo test -p practice-tracks --test native --locked -- --ignored --nocapture
```

GUI installations retain their ordinary display environment; the two Qt platform overrides above are for this headless test. Native Windows/macOS rendering and preference restoration require verification on their respective hosts. Windows renderer code can be type-checked against the Windows APIs, but that alone does not establish native process cleanup or audio compatibility.

The native rehearsal-section test compares an extracted section against an independently
authored standalone score. It covers an earlier pickup, inherited 60 BPM and 3/4 meter,
repeat/jump suppression, immediate playback, ties crossing both boundaries, and natural
decay without the next section's music. It passed with the official MuseScore 4.7.0
AppImage and bundled MS Basic. A separate native region check verified inclusive
measure boundaries, all-staff selection, and inherited key signature and part identity.
Native region export drops earlier tempo annotations and retains external tie references;
section preparation repairs both before applying the existing instrument mixes.
