# Development and standalone builds

Use Python 3.11 or newer. Python 3.12 is used in the native build workflow.

```sh
python -m venv .venv
# Linux/macOS: source .venv/bin/activate
# Windows PowerShell: .venv\Scripts\Activate.ps1
python -m pip install -e . -r requirements-build.txt
python -m pytest -q
python -m build
python -m PyInstaller --clean --noconfirm mxml.spec
python packaging/smoke.py dist/mxml
python packaging/checksum.py dist/mxml
```

On Windows, use `dist/mxml.exe` for the last two commands. PyInstaller builds
for the operating system and architecture running the build. The executable
includes the offline MusicXML schemas and does not require an installed Python.
Build requirements pin direct verification tools; the runtime package declares
its supported `lxml` version range.

The GitHub Actions workflow runs tests, builds Python distributions, builds and
smoke tests standalone executables, and uploads executables with SHA-256 files.
It uses Windows x64, Linux x64, macOS Intel, and macOS Apple Silicon runners.
Artifacts are unsigned. The workflow does not publish releases.

Linux standalone smoke tests and Python wheel/source-distribution builds passed
in this workspace. Other platform executables still require their native CI
runs. Downloaded Unix executables may need `chmod +x mxml-linux-x64` (or the
corresponding macOS filename) after extracting the Actions artifact.

## Session runtime

The workspace initially had Python 3.10 only. A local `.venv` now runs Python
3.12.15, with lxml 6.0.2, pytest 8.4.2, build 1.3.0, and PyInstaller 6.16.0.
Its base interpreter was bootstrapped under `/tmp/mxml-python`; recreate the
environment with your own Python 3.11+ after temporary files are removed.

## Codex tooling

Exact reviewed versions and commits are recorded in `tools/tooling-lock.json`.
These tools support development; they are not executable runtime dependencies.
Existing unrelated Codex configuration and graph indexes were preserved.

| Tool | Installed state | Activation state |
| --- | --- | --- |
| Caveman | Official Codex installer installed v4.0.1 skills at commit `d262ab3882952a59312ff3537cd86d894523c5ab`. The existing Caveman CLI is 1.3.4 (binary release bin-v1.1.7). | Existing native Codex hooks were retained. Caveman instructions are active in this session. New skills become available on the next turn; the official installer skipped duplicate hooks. |
| Ponytail | Existing enabled Codex plugin is 5.3.2, checkout `09c434ab73ca43761c5d8b0c5d341926c2fba710`. It already matched the reviewed pin, so no replacement was needed. | Existing lifecycle hooks have trusted hashes in Codex configuration. Ponytail instructions are active in this session. |
| codebase-memory-mcp | Existing 0.12.1 binary retained; native `install --clients=codex -y` refreshed its Codex MCP configuration, skill, agent profiles, and hooks. | MCP tools and graph context are available in this session. The installer requires a session restart to reload refreshed integration; review changed hooks in `/hooks` if requested. |

Installation and hook activation are separate. Codex hook trust belongs to the
user. The installers do not bypass `/hooks` review. A new session is needed to
prove newly installed lifecycle hooks run; existing injected instructions and
MCP calls prove only current-session availability.

Reproduce the official Caveman installation from the pinned installer checkout:

```sh
git clone https://github.com/JuliusBrussee/caveman.git
git -C caveman checkout bad48a0e1df55e05e3f52249afa35ecc1909ee76
node caveman/installer/install.js --only codex --no-mcp-shrink
```

The installer selects Caveman release v4.0.1. `--no-mcp-shrink` preserves the
existing MCP/output configuration. The upstream Codex installation documentation
is [Caveman INSTALL.md](https://github.com/JuliusBrussee/caveman/blob/bad48a0e1df55e05e3f52249afa35ecc1909ee76/INSTALL.md).

For a fresh Ponytail setup, its documented marketplace supports a pinned ref:

```sh
codex plugin marketplace add DietrichGebert/ponytail --ref 09c434ab73ca43761c5d8b0c5d341926c2fba710
codex plugin add ponytail@ponytail
```

Do not replace an existing marketplace merely to rerun this command. The current
checkout already has the pinned content. See
[Ponytail INSTALL.md](https://github.com/DietrichGebert/ponytail/blob/09c434ab73ca43761c5d8b0c5d341926c2fba710/INSTALL.md).

Install a fresh codebase-memory binary from the
[v0.12.1 release](https://github.com/DeusData/codebase-memory-mcp/releases/tag/v0.12.1),
verify its release checksum, then run:

```sh
codebase-memory-mcp install --clients=codex -y
```

Review `/hooks` and restart Codex after installing or updating hooks. Reindex the
project once source files exist and after material changes.

## Offline MusicXML validation

`src/mxml/schema/` bundles the official MusicXML 4.0 XSD and its two imports.
Only the two import locations were changed to relative local filenames.
Copyright and license notices remain in the schemas; `NOTICE.md` records source
and provenance. Schema compilation was verified with XML networking disabled.

## Rendered inspection

MuseScore 3.6.2 rendered a representative two-measure SATB score and its split
and merged results. The images were visually inspected: the split keeps the
original Soprano with two numbered staves immediately beneath it, copies the
shared first measure, and separates later chord tones. The merged score has two
staves, shared notes, opposing stems after divergence, separate differing lyrics,
and a bass clef for the lower pair.

Local inspection artifacts are `dist/preview.musicxml`,
`dist/preview-split.musicxml`, `dist/preview-merged.musicxml`, and their
`preview-original-1.png`, `preview-split-1.png`, `preview-merged-1.png` renders.
These generated files are ignored by Git. The renderer was bootstrapped under
`/tmp` from the official MuseScore AppImage and extracted runtime libraries;
no system packages were installed. This representative inspection does not
replace checking a user's complete score in their preferred notation program.

The session used the official `MuseScore-3.6.2.548021370-x86_64.AppImage`,
extracted under `/tmp/squashfs-root`. Its Qt X11 plugin ran against an isolated
Xvfb display `:94`; missing shared libraries were extracted under
`/tmp/mxml-musescore-libs/root`. A temporary Xvfb loader shim relocated its
keyboard compiler into that same directory. The temporary MuseScore preferences
disabled `export/png/useTransparency` so previews have a white background.
The successful export command was:

```sh
XDG_CONFIG_HOME=/tmp/mxml-ms-config \
DISPLAY=:94 \
LD_LIBRARY_PATH=/tmp/mxml-musescore-libs/root/usr/lib/x86_64-linux-gnu \
/tmp/squashfs-root/AppRun \
  -o /home/Codex/MXML-CLI/dist/preview-merged.png \
  /home/Codex/MXML-CLI/dist/preview-merged.musicxml
```

These temporary paths describe this session, not an additional project runtime
dependency. A regular local MuseScore installation can export the same files.
