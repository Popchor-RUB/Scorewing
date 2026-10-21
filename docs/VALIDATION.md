# Validation

Rust unit/integration tests cover musical timing, transformations, file safety, CLI behavior, and desktop session validation. Frontend tests cover selection and merge-group behavior. The existing Python tests are retained as a migration oracle and can run the Rust CLI for transformation assertions through `tests.rust_parity`.

```sh
tools/devcontainer.sh run bash tools/check.sh
```

The shared engine embeds the official MusicXML 4.0 schemas, with imports restricted to local bundled files. Output validation requires no Python installation and no network access. Schema licensing and provenance remain alongside the schema files.

CI builds unsigned native artifacts for Linux x64, Windows x64, macOS Intel, and macOS Apple Silicon. Native builds and interactive GUI behavior on platforms unavailable locally must be verified by their respective runners; a configured workflow alone does not establish that those builds passed.

The Linux Podman verification passed 51 Rust tests, 73 Python reference tests, 42 Rust-engine parity cases, frontend workflow tests, frontend type checking, and strict workspace Clippy checks. The frontend dependency audit reported no vulnerabilities. The release app was launched under Xvfb and its real native Open dialog loaded the existing four-instrument sample. DOM tests additionally simulate native file-drop and export-dialog responses.

Inputs used in previous manual musical inspection are not committed. No page-by-page engraving review, playback check, signing, or notarization is claimed by the automated suite.
