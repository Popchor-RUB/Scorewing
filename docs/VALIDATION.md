# Validation

Rust unit/integration tests cover musical timing, transformations, file safety, CLI behavior, and desktop session validation. Frontend tests cover selection and merge-group behavior. All former Python test scenarios are covered directly in Rust; the Python reference implementation and parity driver have been removed.

| Coverage | Rust test location |
| --- | --- |
| Exact timing, chords, slicing, relationship styles, invalid scores | `crates/mxml-core/src/model.rs` unit tests |
| Plain/compressed roundtrips, archive safety, schema validation, atomic output protection | `crates/mxml-core/src/io.rs` unit tests |
| Split ordering, voice/chord sharing, ties/slurs, annotations, metadata and identifiers | `crates/mxml-core/tests/split.rs` |
| Merge ordering, rhythms, rests, lyrics, instruments, compatibility and relationships | `crates/mxml-core/tests/merge.rs` |
| CLI split/merge roundtrips, mixed name/position selectors, invalid groups and file protection | `crates/mxml-cli/tests/cli.rs` |

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
