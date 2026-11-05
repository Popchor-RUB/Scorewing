# Scorewing update window

Status: design approved and implemented on November 4, 2026. The native update window uses the revised compact height.

Open [the unified gallery](../index.html). Edit page **06 · Update window** in [the single Figcraft source](../scorewing.figcraft). Frames 24–26 are exported to `../screens/` at 2× and listed in [the screen inventory](../README.md).

## Captured states

| Frame | Export | State |
| --- | --- | --- |
| 24 | [`24-update-available-light.png`](../screens/24-update-available-light.png) | Update available, light appearance |
| 25 | [`25-update-available-dark.png`](../screens/25-update-available-dark.png) | Update available, dark appearance |
| 26 | [`26-update-installation-disabled.png`](../screens/26-update-installation-disabled.png) | Open score, pending selection, or work prevents installation |

Each frame uses a 620 × 248 logical-pixel content area, reduced from the previous 620 × 340 non-resizable Tauri update window. The operating system supplies its title bar, titled “Scorewing update available”; window decorations are excluded because they vary by platform.

Layout: 24px outer padding, 80px application icon, 24px content gap, text beginning at x=128, and actions aligned to the bottom right with 8px gaps. The button row moves from y=285 to y=193, removing 92px of blank space. This leaves 40px between the normal guidance and buttons, or 21px with the longer disabled-state guidance. Colors follow the application’s light and dark semantic tokens. Inter is an available design-font substitute for the runtime’s platform-dependent system font. Text wrapping and button widths can therefore differ slightly by platform.

The illustrated versions, 0.1.8 and 0.1.9, are sample data and do not assert that an update has been published.

## Existing behavior and component ownership

- Cancel dismisses this prompt; Escape also cancels.
- Skip this version remembers the offered version and suppresses its automatic prompt. A manual check can still offer it.
- Download & Install downloads, installs, and restarts the app. It is disabled while a score or selection is open or work is running.
- View changelog on GitHub opens the latest release page.
- The disabled state explains that closing a score discards its editing choices.
- Actions use the application-owned shadcn-style Button built on Base UI. The window content is a custom section with dialog semantics inside a separate Tauri window; it does not currently use the shared Dialog component.

This captures the update-available window only. Checking, download progress, installation, and error dialogs remain in the existing main-window update flow.

## Source and verification

Sources: `apps/desktop/src/update-prompt.tsx`, `apps/desktop/src/update-control.tsx`, `apps/desktop/src/style.css`, `apps/desktop/src/components/ui/button.tsx`, `apps/desktop/src/app.tsx`, and `apps/desktop/src-tauri/src/update_prompt.rs`.

Created editable text, image, and button layers through Figcraft MCP; inspected layer geometry and the rendered three-state preview; confirmed the design font is available. Export dimensions and gallery references were checked during the initial design capture.

Implementation changes only the native window height from 340 to 248 logical pixels. The existing viewport-filling flex layout moves the buttons upward without CSS or component changes and retains scrolling for longer messages. All 10 focused tests in `update-prompt.test.ts` and `updater.app.test.ts` passed, as did `git diff --check`. The test harness logged non-fatal Vite HMR port conflicts. Rust formatting could not run because `rustfmt` is not installed. Native GUI appearance and a live update were not exercised.

The inline review was rendered from the Figcraft exports. T3's HTML browser preview could not run because the host lacks required browser libraries; the Figcraft screenshot and artifact checks succeeded.
