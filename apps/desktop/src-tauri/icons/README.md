# Application icons

`../../app-icon.svg` uses the supplied `Scorewings-vector.svg` artwork. Its separate black canvas layer was removed with Vectorcraft MCP for transparent corners; the rounded blue background remains.

`../../app-icon-macos.svg` uses the same artwork at 80.46875% scale, centered, preserving the existing macOS footprint (100px margins on a 1024px canvas).

Artwork edits and resizing use Vectorcraft MCP. Both SVGs have a 1254×1254 artboard. To regenerate the desktop assets:

1. Open `app-icon.svg` and export PNGs at 16, 24, 32, 48, 64, 128, 256, and 512px with `export` (`scale: size / 1254`).
2. Keep the 32px, 128px, 256px, and 512px renders as `32x32.png`, `128x128.png`, `128x128@2x.png`, and `icon.png` respectively. Package the 16, 24, 32, 48, 64, and 256px PNG frames into `icon.ico` without resizing.
3. Open `app-icon-macos.svg` and export PNGs at 32, 64, 128, 256, 512, and 1024px. Package these into `icon.icns`, using `ic11` (32px), `ic12` (64px), `ic07` (128px), `ic08`/`ic13` (256px), `ic09`/`ic14` (512px), and `ic10` (1024px), without resizing.

Only the desktop assets are kept. The bundle configuration already references the Linux PNGs, Windows ICO, and macOS ICNS.
