# Scorewing update window

Status: channel-aware design approved by the user and implemented on November 6, 2026.

Open [the unified gallery](../index.html). Edit page **06 · Update window** in [the single Figcraft source](../scorewing.figcraft). Frames 24–45 are exported at 2× and listed in [the screen inventory](../README.md). Each frame is 620 × 328 logical pixels; native title bars are excluded.

## Layout and controls

Retain 24 px outer padding, the 80 px application icon, content beginning at x=128, and bottom-right actions. The title is “Scorewing updates”. Add a labeled application-owned shadcn Select for **Release channel**, with Stable and Nightly choices, beneath the title. Retain semantic light/dark colors and shared Button styling. Inter substitutes for the runtime system font.

Changing the channel remembers the choice across restarts and starts a fresh check immediately. The first selection follows the installed build's channel. A manual Check for updates always opens this separate, nonblocking window, including while checking, when current, offline, or unsupported. Startup checks stay quiet unless a newer, unskipped version is available.

## Captured states

- 24: [Update available · Light](../screens/24-update-available-light.png).
- 25: [Update available · Dark](../screens/25-update-available-dark.png).
- 26: [Installation disabled · Light](../screens/26-update-installation-disabled.png).
- 27: [Installation disabled · Dark](../screens/27-update-installation-disabled-dark.png).
- 28: [Checking for updates · Light](../screens/28-update-checking.png).
- 29: [Checking for updates · Dark](../screens/29-update-checking-dark.png).
- 30: [Up to date · Light](../screens/30-update-up-to-date.png).
- 31: [Up to date · Dark](../screens/31-update-up-to-date-dark.png).
- 32: [Update check failed · Light](../screens/32-update-check-error.png).
- 33: [Update check failed · Dark](../screens/33-update-check-error-dark.png).
- 34: [Downloading update · Light](../screens/34-update-downloading.png).
- 35: [Downloading update · Dark](../screens/35-update-downloading-dark.png).
- 36: [Update ready · Light](../screens/36-update-ready.png).
- 37: [Update ready · Dark](../screens/37-update-ready-dark.png).
- 38: [Installing update · Light](../screens/38-update-installing.png).
- 39: [Installing update · Dark](../screens/39-update-installing-dark.png).
- 40: [Older channel version · Light](../screens/40-update-older-version.png).
- 41: [Older channel version · Dark](../screens/41-update-older-version-dark.png).
- 42: [Confirm downgrade · Light](../screens/42-update-confirm-downgrade.png).
- 43: [Confirm downgrade · Dark](../screens/43-update-confirm-downgrade-dark.png).
- 44: [Manual download required · Light](../screens/44-update-unsupported-install.png).
- 45: [Manual download required · Dark](../screens/45-update-unsupported-install-dark.png).

The window remains the same surface through checking, available/current results, errors, download progress, readiness, installation, manual downloads, and downgrade confirmation. The selected channel's release-notes link opens the corresponding stable or nightly GitHub release. Versions shown are illustrative.

## Behavior and invariants

- Closing/Cancel dismisses the window; downloading may continue after Hide. Installation cannot be interrupted by a window action.
- Skip this version remembers the version per channel; manual checks override skipping.
- The release-channel control is disabled during download/installation. On a channel change, discard the previous resource, progress, errors, and confirmation; late responses cannot replace the newly selected result.
- Close scores and score selections and finish active work before installing. Closing a score discards its editing choices. Existing frontend and backend installation guards remain authoritative.
- A manually requested older channel version first offers **Review downgrade**, then a separate confirmation state with **Back** and **Downgrade & restart**. Startup never offers an automatic downgrade.
- Errors keep the window open and offer a suitable retry. A pruned nightly asset requires a fresh check. Unsupported Linux installations offer GitHub downloads.
- Main-window updater orchestration retains ownership of updater resources and install safety. The auxiliary window displays state and sends actions; it does not gain independent installation authority.

## Source and verification

Design source and all exports were updated through Figcraft MCP and inspected before review. Gallery and screen inventory match all 45 frames. Source: `apps/desktop/src/update-prompt.tsx`, `apps/desktop/src/update-control.tsx`, `apps/desktop/src/style.css`, `apps/desktop/src-tauri/src/update_prompt.rs`, and backend updater commands.

The previous November 4 design captured only the available-update prompt at 620 × 248. The approved design consolidates all existing update states into this window and adds channel selection and explicit downgrades. The frontend and backend now implement the approved flow.

Validation ran in Podman: all 63 frontend tests passed, TypeScript checking and the production frontend build passed, and both focused Rust updater tests and desktop Clippy passed. Release tooling passed 63 tests, with the existing PowerShell-only test skipped on Linux. The source, 45-frame gallery/inventory, and PNG dimensions were cross-checked. Live GitHub publication, native installer upgrades/downgrades, and macOS signing/notarization were not exercised locally.
