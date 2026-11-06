<p align="center">
  <img src="apps/desktop/src-tauri/icons/icon.png" alt="Scorewing icon" width="160" height="160">
</p>

# Scorewing

[![Latest release version](https://img.shields.io/github/v/release/Popchor-RUB/Scorewing?label=latest%20release)](https://github.com/Popchor-RUB/Scorewing/releases/latest)

**Prepare choir scores and practice tracks with a few clicks.** Scorewing helps you separate voices, combine staves, highlight changes between score versions, and create MP3s for rehearsing each part.

![Scorewing tool selection screen with Split instruments, Merge instruments, Practice Tracks, and Diff versions](docs/design/screens/02-tool-chooser.png)

## Download and install

Get the latest version from the **[download page](https://github.com/Popchor-RUB/Scorewing/releases/latest)**. Choose the application for your system rather than the source-code archives.

| System | Download and install |
| --- | --- |
| Windows | Download the `.exe` installer and run it. The app requires Microsoft Edge WebView2. |
| macOS | Download the `macos_universal.app.tar.gz` archive, extract it, and move **Scorewing.app** to **Applications**. It supports both Apple silicon and Intel Macs. |
| Linux | Download the `.AppImage`, allow it to run as a program in its file properties, and open it. The app requires Ubuntu 24.04-compatible GTK/WebKitGTK libraries. |

To work with MuseScore files or create practice tracks, install **MuseScore Studio 4.7.x** separately. Practice tracks also use its bundled **MS Basic** SoundFont. Scorewing looks for these automatically; you can set their paths in the renderer settings if needed. MusicXML splitting, merging, and comparison do not require MuseScore.

## Get started

1. Open Scorewing and choose **Open score**, or drop a score into the window.
2. Choose a tool, select the instruments and options you need, and export the result.
3. Open the exported score in your notation app, or listen to your practice MP3s in your usual audio player.

Supported inputs are MusicXML (`.musicxml`, `.xml`, `.mxml`, or compressed `.mxl`) and MuseScore (`.mscz` or `.mscx`). MuseScore files open in Practice Tracks by default. You can also drop two score versions together to compare them.

## Tools

### Split instruments

Separate voices or chord tones from a shared staff into individual staves—for example, turn a combined soprano and alto staff into separate parts. Choose which instruments to split and, where needed, which voice should fill in when another is missing. The exported score keeps the originals and adds the separated staves below them.

### Merge instruments

Combine two or more instruments onto a shared staff, such as bringing soprano and alto together for a compact choir score. Create your groups and choose up or down stems for each source. The export adds the merged staves while keeping the original instruments.

### Practice Tracks

Create an MP3 for each selected part from a MuseScore 4 score, with that part in the foreground and your chosen accompaniment quieter in the background. Pick instrument sounds, adjust the balance, and optionally add a metronome. Export the whole song, individual rehearsal sections, or both.

### Diff versions

Compare an older and a newer score and export a copy of the newer version with changes marked in red. Scorewing highlights differences in notes, rhythms, lyrics, dynamics, and more, helping singers find what changed before the next rehearsal. Check or swap the version order before exporting; MusicXML and MuseScore files can be compared with each other.

## Your files and results

Scorewing leaves your source scores unchanged and asks before replacing existing output files. Split and merge keep all original instruments, including hidden ones. If an export reports warnings, review the affected passages in your notation app.

Split and merge transform single-staff instruments; multi-staff instruments remain in the export but cannot be selected. Diff versions does not currently support multi-staff instruments. Scorewing does not display notation or play audio itself.

For practice tracks, keep MuseScore closed during export so Scorewing can temporarily manage its metronome setting and restore it afterward. If an export is cancelled or fails, completed MP3s are retained and their locations are reported.

## Updates and further help

Scorewing checks for updates at startup and asks before downloading. To check manually, choose **Check for updates…** from the **Scorewing** menu on macOS or the **Help** menu on Windows and Linux. Close your score before choosing **Install and restart**.

- [Detailed usage and technical reference](docs/USAGE.md): tool options, rehearsal-section behavior, export warnings, and command-line examples.
- [Development and build setup](docs/SETUP.md): build Scorewing yourself or contribute to development.
- [Validation and platform limits](docs/VALIDATION.md): tested behavior and remaining platform checks.
- [Report a problem or suggest an improvement](https://github.com/Popchor-RUB/Scorewing/issues).
