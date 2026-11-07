# Scorewing update window

Status: channel-aware design approved and implemented on November 6, 2026. The November 7 split-button, default Progress, cancellation, and deferred-install revision was approved by the user and is implemented.

Open [the unified gallery](../index.html). Edit page **06 · Update window** in [the single Figcraft source](../scorewing.figcraft). Frames 24–51 are exported at 2× and listed in [the screen inventory](../README.md). Each frame is 620 × 328 logical pixels; native title bars are excluded.

## November 7 revision — approved and implemented

- Replace the native HTML download progress element with the default `base-nova` shadcn Progress: 4 px `bg-muted` track, `bg-primary` indicator, accessible label, and indeterminate behavior when the total size is unknown. Preserve the application theme; no green success styling or custom progress colors. In both immediate and background download views, place the release-notes link above the “Downloading” status text and omit the visible percentage label. Keep the actual progress value available to assistive technology.
- For a supported newer release, use shadcn ButtonGroup, Button, and DropdownMenu. The left action remains **Download & Install**. The right ChevronDown button has the accessible name **More update options**; its single grouped item is **Install on next start**. Open the menu upward at the bottom of the window, aligned to the right edge. Retain keyboard navigation, focus return, and Escape dismissal through the shadcn primitives.
- Choosing **Install on next start** closes the update window immediately and starts a background download. It never installs or restarts the current session. Immediate installation remains disabled while scores/selections or active work exist; the arrow/menu choice remains enabled because downloading is safe during current work. Do not offer deferred downgrades or unsupported-platform installs.
- In all downloading states (frames 34–35 and 48–49), show a red **Cancel** button using the existing shadcn `destructive` variant and a separate outline **Hide** button. Use the application’s semantic destructive color in both themes, with the existing contrasting label color. Cancel aborts the active download, discards the incomplete installer and signature, clears deferred-install intent, and returns to the available-update state so the user can retry. It never installs or restarts. Cancellation must invalidate the active attempt before cleanup so late progress/completion callbacks cannot schedule or install it. Hide, native window close, and Escape only dismiss the window and keep the download running. Cancellation is available only during download; an installation already in progress cannot be interrupted.
- Frames 48–51 show the update window only after the user manually reopens it. Downloading continues after Hide; completion stays quiet. The scheduled state has only **Close**, and explicitly says installation will happen on next start. Closing the window does not cancel the staged update. No extra completion modal or notification is introduced.
- Persist the exact downloaded installer bytes and detached release signature with required version/platform metadata in application-owned storage. The signature may come from the signed-update feed; retain its exact value. Publish a scheduled update only after a complete, verified download and an atomic save; interrupted/partial downloads must never become installable. Keep native ownership of the staged update and prevent concurrent staging/install attempts.
- On the next application launch, before opening scores or running update checks, load the staged package and signature, reject invalid metadata or obsolete/incompatible versions, and verify the exact bytes again against the application's embedded trusted updater public key **immediately before installation**. A previous download-time check does not replace this check. Reuse the installed updater's platform installer and verification where its API guarantees this; otherwise perform the required verification in native code. Installation of a complete staged update must not require a fresh network request.
- Fail closed for missing/invalid signatures, modified/truncated bytes, incompatible metadata, or installer errors. Keep the existing application usable; expose the failure through the existing update error surface and a suitable fresh-check/download retry. Failed background downloads must not reopen a modal over the user's work. Do not report an update as scheduled until its durable package and signature are saved. Never install on quit; the next launch is the installation trigger.
- Preserve existing immediate-install guards, channel/revision race handling, and explicit downgrade confirmation. Lock channel selection during background download and while a staged update is scheduled so displayed state cannot refer to a different release. The auxiliary update window remains a display/action surface; it gains no filesystem or independent install authority.

Affected existing frames: 24–27 and 34–35. New frames:

- 46: [Install on next start menu · Light](../screens/46-update-install-on-next-start-menu.png).
- 47: [Install on next start menu · Dark](../screens/47-update-install-on-next-start-menu-dark.png).
- 48: [Background update download · Light](../screens/48-update-background-download.png).
- 49: [Background update download · Dark](../screens/49-update-background-download-dark.png).
- 50: [Update scheduled for next start · Light](../screens/50-update-scheduled.png).
- 51: [Update scheduled for next start · Dark](../screens/51-update-scheduled-dark.png).

The saved Figcraft source, 51-frame inventory/gallery, and 12 affected PNG exports were synchronized. Upstream shadcn Progress, ButtonGroup, and DropdownMenu documentation was fetched before design review. The implementation and focused verification are recorded below.

## November 7 implementation and verification

- The update window uses the upstream shadcn Base UI Progress, ButtonGroup, and DropdownMenu with the existing theme and Button variants. The menu stays enabled while immediate installation is blocked. Cancel stops the transfer; Hide, Escape, and window close only dismiss the window. Download callbacks and promises are invalidated on cancellation so late completion cannot install or schedule a cancelled attempt. Native cancellation also removes a package when completion wins the race; cleanup failures can be retried.
- Native commands own downloads, immediate installation, and deferred staging. They are restricted to the main window. The auxiliary window only sends typed actions. Direct frontend permissions for the updater plugin's download/install commands have been removed. The existing score/work guards remain, with native checks for registered scores and practice jobs and for Linux AppImage support.
- The exact installer bytes and original manifest (including the detached signature) are stored in one bounded, atomically published application-data file. Cancel/consumption synchronizes removal on Unix. A standard-library file lock holds updater ownership across application instances, and is released before the replacement process starts. Downloads preserve the previous ten-minute timeout independently of the fifteen-second check timeout.
- Startup restores the updater with the application's embedded key and current platform configuration, without fetching a manifest or package. The library verifies the saved bytes and the signature's authenticated version before installation; its install method verifies those exact bytes again before entering the platform installer. Missing signatures, altered bytes, unsigned/forged versions, obsolete releases, unsupported platforms, and malformed or partial files fail closed. A failed startup leaves the app usable and exposes a recoverable error through the update surface. No installation happens on quit.
- Compatibility: deferred installation requires a version recorded in the signature's trusted comment, as produced by the current Tauri bundler. Legacy signatures without that authenticated version are rejected for deferred installation. The upstream installer remains responsible for validating the native package format; signatures authenticate package bytes and version, while the saved manifest selects the platform entry.
- Podman verification: all 68 frontend tests passed; TypeScript and the production frontend build passed. All 33 native desktop library tests passed, including seven focused updater tests for persistence bounds, partial files, signature tampering/missing signatures, signed-version checks, obsolete/incompatible metadata, cancellation races/cleanup retries, and exclusive ownership. Desktop Clippy passed with warnings denied.
- Real Chromium checks ran inside Podman in light and dark at 620 × 328. They verified menu placement, keyboard Escape/focus return, blocked immediate installation with an available deferred action, release-notes ordering, no visible percentage, semantic destructive Cancel styling, progress height, separate Cancel/Hide events, and the scheduled Close state without overflow. Runtime screenshots are verification artifacts; the Figcraft document and its 51 exports remain the design references.
- Not exercised locally: a real installer replacement/relaunch on Linux, Windows, or macOS, a live update-server download, or macOS authorization/signing/notarization. The signature/install guard tests use a public throwaway upstream test vector and reject modified bytes before any installer can run.

## Previously implemented layout and controls

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

- Outside a download, Closing/Cancel dismisses the window. During a download, Hide/window close/Escape dismisses the window and continues downloading; the separate Cancel button stops the download as specified above. Installation cannot be interrupted by a window action.
- Skip this version remembers the version per channel; manual checks override skipping.
- In the implemented November 6 flow, the release-channel control is disabled during download/installation. On a channel change, discard the previous resource, progress, errors, and confirmation; late responses cannot replace the newly selected result.
- Close scores and score selections and finish active work before installing. Closing a score discards its editing choices. Existing frontend and backend installation guards remain authoritative.
- A manually requested older channel version first offers **Review downgrade**, then a separate confirmation state with **Back** and **Downgrade & restart**. Startup never offers an automatic downgrade.
- Errors keep the window open and offer a suitable retry. A pruned nightly asset requires a fresh check. Unsupported Linux installations offer GitHub downloads.
- Main-window updater orchestration retains ownership of updater resources and install safety. The auxiliary window displays state and sends actions; it does not gain independent installation authority.

## Source and verification

Design source and all exports were updated through Figcraft MCP and inspected before review. Gallery and screen inventory matched all 45 frames at that implementation checkpoint. Source: `apps/desktop/src/update-prompt.tsx`, `apps/desktop/src/update-control.tsx`, `apps/desktop/src/style.css`, `apps/desktop/src-tauri/src/update_prompt.rs`, and backend updater commands.

The previous November 4 design captured only the available-update prompt at 620 × 248. The approved design consolidates all existing update states into this window and adds channel selection and explicit downgrades. The frontend and backend now implement the approved flow.

Validation ran in Podman: all 63 frontend tests passed, TypeScript checking and the production frontend build passed, and both focused Rust updater tests and desktop Clippy passed. Release tooling passed 63 tests, with the existing PowerShell-only test skipped on Linux. The source, 45-frame gallery/inventory, and PNG dimensions were cross-checked. Live GitHub publication, native installer upgrades/downgrades, and macOS signing/notarization were not exercised locally.
