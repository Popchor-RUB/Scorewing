# Default shadcn fields

Status: approved and implemented on November 6, 2026.

## Audit — November 6, 2026

Compared the desktop frontend with the installed shadcn skill, official Base UI component documentation, and the `@shadcn` registry resolved using the project's `base-nova` style. Before this change, the controls used Base UI in places, but were custom implementations rather than the default shadcn components. The findings below record that starting state; the listed field changes are now implemented.

| Area | Evidence | Required change |
| --- | --- | --- |
| Input | `apps/desktop/src/components/ui/input.tsx` wraps a native input and delegates all styling to `.input`. | Use the default registry Input, which composes Base UI Input and includes focus, disabled, and invalid states. |
| Select | `apps/desktop/src/components/ui/select.tsx` provides custom trigger, popup, indicator, and item markup. Split and Merge render items without SelectGroup. | Use the default registry Select components and wrap item sets in SelectGroup. |
| Combobox | `apps/desktop/src/components/ui/combobox.tsx` implements its own InputGroup, trigger, SVGs, and popup styles. | Use the default registry Combobox and its shadcn InputGroup dependencies. Retain the existing application-level sound selection behavior. |
| Field composition | `practice-view.tsx`, `sound-picker.tsx`, and `split-workspace.tsx` use custom containers and raw labels. Numeric fields lack aria-invalid/data-invalid presentation. | Compose FieldGroup, Field, FieldLabel, FieldDescription, and FieldError where applicable. Reflect disabled and invalid state on fields and controls. |
| Theme and styles | `style.css` manually styles each control and popup. Its Tailwind theme omits input, ring, popover, and several foreground tokens; accent maps to primary. | Wire the semantic tokens required by upstream components and remove superseded field CSS. Keep the established desktop theme and density in shared theme tokens. |
| CLI configuration | `npx shadcn@latest info --json` and `docs` fail because components.json aliases cannot be resolved. | Configure valid aliases in components.json, TypeScript, and Vite so the CLI and application agree. |

The search confirmed that application text/number inputs use Input, predefined dropdowns use Select, and sound search uses Combobox; no additional raw input/select/textarea controls were found outside the shared UI components. File picking uses native dialogs and remains separate.

This is a field-focused audit, not certification of every frontend rule. For example, the update dialogs still use `space-y-3`, and other shared UI components also have custom implementations.

## Approved design

Use the default shadcn Base UI `base-nova` Input, Select, and Combobox structure and states, with the existing semantic desktop palette and text density. Fields use a shared 32 px height and 8 px corners. The editable sound control grows from 28 px to 32 px while retaining its center and row placement. The open sound list uses the same rounded popup and the application labels “Recently selected” and “All instruments”. Labels, validation, and disabled presentation belong to the standard Field composition. Browse and Set Default remain explicit adjacent actions.

Updated frames: 03–05, 07, 09–19, and 21–23. This includes the design foundations, dark and compact variants, renderer settings, open sound search, and workspaces underneath dialogs. All frames remain in the existing scorewing.figcraft document; their current PNGs and gallery captions are synchronized.

## Implementation

- Replaced Input, Select, and Combobox from the official `@shadcn` registry using the configured `base-nova` style and added its Field, InputGroup, Label, Separator, and Textarea dependencies. Components retain their upstream structure and styles; the Combobox trigger has an explicit accessible name. The existing Button gains the small icon size needed by Combobox, preserving the rest of the desktop button styling.
- Updated Split, Merge, Practice, and SoundPicker to use standard field composition, SelectGroup, associated labels/descriptions, and disabled/invalid attributes. Numeric range errors and unavailable sounds expose linked FieldError messages. Width and placement remain layout concerns.
- Fixed CLI aliases in components.json, TypeScript, and Vite. Added shared semantic theme tokens and desktop density tokens; removed obsolete field CSS and moved the general focus rule into the base layer so component focus styles take precedence. Added pinned cn, class-variance-authority, lucide-react, and tw-animate-css dependencies and updated the lockfile.
- Updated markup-dependent test selectors and added assertions for grouped options, accessible triggers, numeric boundaries, linked errors, and invalid/disabled fields.

Preserve preset identity by bank/program, duplicate display names, case-insensitive filtering, shared persistent recents, keyboard and mouse selection, no clear button, query cancellation on Escape/blur/Tab, explicit Set Default, renderer path saving on blur, numeric ranges, unique accessible labels, native file browsing, and busy/role-based disabling. The existing application and sound-picker tests remain in place and pass.

Sources: [Input](https://ui.shadcn.com/docs/components/base/input), [Select](https://ui.shadcn.com/docs/components/base/select), [Combobox](https://ui.shadcn.com/docs/components/base/combobox), [Field](https://ui.shadcn.com/docs/components/base/field), and [Input Group](https://ui.shadcn.com/docs/components/base/input-group).

## Verification

- Production frontend build passed in Podman (`tsc --noEmit` and Vite).
- All 58 frontend tests passed in Podman, including the updated field assertions.
- `npx shadcn@latest info --json` now succeeds, identifies Vite and Base UI, and lists the installed field components.
- `git diff --check` passed. The 26-frame inventory, gallery entries, and PNG filenames agree.
- Real-browser verification could not run: the existing Podman image lacks Chromium's NSS/audio libraries, and the disposable container's Ubuntu repositories failed package signature verification. No signature checks were disabled. Light/dark and compact design exports were inspected; application behavior is covered by the frontend tests above.
- npm audit reports a pre-existing high-severity advisory for the unchanged happy-dom development dependency (20.14.6); none of the newly added dependencies is flagged. Updating that test dependency is outside this field replacement.
