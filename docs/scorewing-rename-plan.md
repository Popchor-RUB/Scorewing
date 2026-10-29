# Scorewing rename plan

Status: implementation applied with three subagents and independent review. Available checks passed; Rust/native verification remains blocked by missing host build prerequisites, as recorded below.

## Agreed scope

- Rename the application from its previous MXML-CLI/Voiceweaver branding to **Scorewing**.
- Rename branded Rust crates, executable names, imports, frontend package metadata, and the connected build/release/documentation references.
- Keep the neutral `practice-tracks` crate and its directory/imports.
- Preserve the existing icon artwork and bundled image assets. Remove incidental old-name SVG metadata only.
- Do not change settings keys or implement migration code.
- Make changes in this checkout. Provide a separate checklist for hosted repository and release changes; do not perform those changes as part of implementation.
- Execute the implementation with three subagents under a coordinating agent, followed by an independent review subagent. Preserve the agreed naming and compatibility scope.

## Naming map

| Surface | Current identity | Target |
| --- | --- | --- |
| Application name, window/header/title | Voiceweaver | Scorewing |
| CLI crate and directory | `mxml-cli`, `crates/mxml-cli` | `scorewing-cli`, `crates/scorewing-cli` |
| CLI executable | `voiceweaver` | `scorewing` |
| Core crate and directory | `mxml-core`, `crates/mxml-core` | `scorewing-core`, `crates/scorewing-core` |
| Core Rust import | `mxml_core` | `scorewing_core` |
| Desktop Rust package | `mxml-desktop` | `scorewing-desktop` |
| Desktop executable | `voiceweaver-desktop` | `scorewing-desktop` |
| Desktop Rust library/import | `mxml_desktop_lib` | `scorewing_desktop_lib` |
| Frontend npm package | `mxml-desktop` | `scorewing-desktop` |
| macOS application bundle | `Voiceweaver.app` | `Scorewing.app` |
| Release artifact prefix | `voiceweaver-` | `scorewing-` |
| Build/container branding | Project-specific `mxml`/`voiceweaver` names | Corresponding `scorewing` names |

Use `Scorewing` in prose and product labels, `scorewing` in executable/package/path names, `scorewing_*` in Rust imports, and `SCOREWING_*` in renamed build environment variables.

## Deliberate compatibility exceptions

These exceptions supersede the initial blanket replacement request. They preserve existing state without adding a migration system:

- Keep Tauri's internal identifier `org.mxml.desktop` in `apps/desktop/src-tauri/tauri.conf.json`. Keep existing window labels and webview origin/storage configuration.
- Keep the existing localStorage keys in `recent-scores.ts`, `recent-sounds.ts`, and `practice-defaults.ts` exactly as they are, including their current readers/writers.
- Keep the existing recovery directory derivation, recovery journal names, and synchronization filenames. In particular, retain `.voiceweaver-metronome.lock` in `crates/practice-tracks/src/preferences.rs`. Replacing it would let old and new versions acquire different locks while changing the same MuseScore preferences.
- Keep the existing `practice-tracks` crate identity.
- Preserve `MusicXML`, the supported `.mxml` extension, format-related fixtures, and third-party MuseScore names. They are format/dependency names, not project branding.
- Historical source/target names in this planning document are intentional. Do not rewrite Git history, `.git` internals, existing user files, caches, or installed applications.

The result intentionally retains a few old internal names. It does not promise a source tree containing zero occurrences of every historical name.

## Subagent execution

When implementation is requested, the coordinating agent records the baseline, assigns the following disjoint ownership, and starts the three implementation workers in parallel. With the coordinator, this uses the available four concurrent agent slots. All workers use the same checkout; separate branches or worktrees are unnecessary.

| Subagent | Owned files and responsibilities |
| --- | --- |
| Rust worker | Root `Cargo.toml` and `Cargo.lock`; the CLI/core crate directories before and after their moves; `crates/practice-tracks/**`; and `apps/desktop/src-tauri/**` except `tauri.conf.json` and icon assets. Own crate/directory/import/binary/library renames, Rust-facing branding, affected fixtures/tests, and native-test environment-variable references. Preserve the neutral practice crate, recovery paths, and lock names. |
| Desktop worker | `apps/desktop/**` outside `src-tauri`, plus `apps/desktop/src-tauri/tauri.conf.json`. Own frontend package metadata/lockfile, product and window titles, visible UI text, and incidental SVG metadata. Preserve settings keys/readers/writers, internal app identifier, webview configuration, existing HeroUI components, and icon artwork. Do not modify bundled image assets. |
| Tooling/docs worker | `tools/**`, `.github/**`, `.devcontainer/**`, `.cargo/**`, root build/container configuration except Cargo manifests/lockfiles, README, UI-PLAN, and `docs/**` except this plan. Own build/package/CI/artifact references, branded build variables/caches, script tests, and documentation. Preserve installer rollback behavior and the separate external checklist. |

The coordinator owns this plan, baseline evidence, cross-worker coordination, integration fixes, final verification, and the final handoff. A newly discovered affected file outside a worker's ownership must be assigned explicitly before editing it. Workers must not edit another worker's files to repair a temporary cross-task inconsistency.

Every worker brief must include the naming map, all compatibility exceptions, its exact file ownership, and the shared symbol/environment-variable names agreed here. State explicitly that other agents are editing the checkout: do not revert their edits, accommodate them, and report cross-scope needs to the coordinator. Use the codebase-memory skill for structural tracing, verify index coverage, and read any reported gaps. The desktop worker must use the HeroUI skill and MCP for frontend work. No worker may spawn further agents, commit, publish, change remotes, remove old installations, or run destructive cache cleanup.

Execution has four phases:

1. **Baseline and dispatch:** the coordinator records Git status and icon checksums, inventories existing names and compatibility exceptions, checks relevant graph coverage, and gives workers the resulting evidence and shared naming contract. Each worker traces its affected readers/callers before editing. Preserve any pre-existing user changes.
2. **Parallel implementation:** workers perform the changes below within their ownership. The Rust worker alone updates `Cargo.lock`; the desktop worker alone updates `package-lock.json`. Preserve dependency versions. Workers report changed files, checks actually run, remaining references/exceptions, and any blockers. Do not run workspace-wide checks against another worker's incomplete edits.
3. **Integration and proof:** wait for all workers to finish, then inspect the combined diff and resolve mismatches. The coordinator runs the acceptance checks once against the integrated result, delegating focused repairs to the original owner when useful. Repeat only checks affected by a repair. Never claim a native platform check passed when the required runner is unavailable.
4. **Independent review and handoff:** after integration and checks, start a read-only review subagent in a freed slot. Review the combined diff and relevant connected code for missed callers, incorrect package/artifact names, settings/app-identity changes, recovery/lock regressions, unintended icon changes, dependency churn, and unclassified legacy references. Require concrete findings with file locations and reasons. The coordinator fixes small findings directly or returns larger fixes to their owner, then runs the relevant checks. Report the final diff, proof, unresolved native checks, and commit proposal; do not commit automatically. No extra review agent is needed unless a material unresolved finding warrants another review.

## Implementation order

The workers apply steps 2–5 in parallel within the ownership above. The coordinator owns steps 1 and 6; the independent review follows integration and verification.

1. **Record the baseline.** Confirm Git status and save checksums for bundled icon images. Inventory tracked old-name variants, including hidden CI/devcontainer files and filenames. Classify matches against the exceptions above before replacing anything. Git is already initialized in this checkout.

2. **Rename crates and wire their consumers together.** Move `crates/mxml-cli` and `crates/mxml-core` to their target directories. Update root workspace/default members, Cargo package names, dependency names/paths, all Rust imports, and desktop `src/main.rs`. Update relative fixture paths affected by directory moves. Update local package/dependency entries in `Cargo.lock` and npm root package metadata in `package-lock.json`, preserving third-party dependency versions.

3. **Rename commands and product text.** Update CLI command metadata, version/help output, diagnostic prefixes, and integration-test executable lookup to `CARGO_BIN_EXE_scorewing`. Update the desktop binary/library targets. Change the Tauri product name/window title, HTML title, app header, practice view text, and startup diagnostics to Scorewing. Retain existing HeroUI components and layout. Update only non-rendered old-name icon SVG metadata; do not regenerate icons or change artwork.

4. **Update tooling and distribution references.** Cover `tools/check.sh`, platform build scripts, Linux packaging/linkage checks, local-build tests, devcontainer files, and `.github/workflows/build.yml`. Match renamed Cargo package selectors, binaries, application bundles, staging paths, smoke checks, and artifact names. Rename brand-derived build environment variables (`MXML_*` and `VOICEWEAVER_NATIVE_PROFILE`) to `SCOREWING_*`, updating their callers, tests, and docs together. Rename branded container images, cache/volume names, and temporary build paths. Leave existing caches/volumes on disk; new names may cause a cold rebuild. Preserve installer staging, verification, backup, and rollback behavior.

5. **Update project documentation.** Revise README, UI-PLAN, setup/validation docs, command examples, package paths, environment variables, and release instructions. Explain the retained internal compatibility identities and how existing automation must adopt the new commands/build variables. Do not add legacy executable aliases or environment-variable fallback logic. Keep existing score files and format handling unchanged.

6. **Verify the rename and review its scope.** Run the checks below, inspect the final diff for unintended data/identity changes, and classify every remaining tracked old-name match. Prepare a commit proposal; do not commit automatically.

## Acceptance checks

- Run existing tooling tests with `node --test tools/*.test.mjs`.
- In `apps/desktop`, run the existing frontend tests and production build (`npm test`, `npm run build`).
- Run Rust formatting verification, locked workspace tests, and the repository's existing Clippy checks in its supported build environment. Use `cargo fmt --all -- --check` and `cargo test --workspace --locked` as the workspace gates; reuse project tooling rather than introduce a second check framework.
- Build/run the renamed CLI and assert `scorewing --version`, help text, and existing integration tests use the new identity. Verify existing CLI behavior through the current tests.
- Build platform packages using the existing scripts/CI. Check Scorewing bundle names, renamed executables, artifact contents, and the existing linkage/package checks. Native packaging/installer checks require the corresponding OS runner.
- Confirm settings-key definitions, Tauri identifier, recovery paths, and lock filenames are unchanged. No migration readers, aliases, or cleanup jobs should be added.
- On native packaged upgrade checks, seed recent scores, recent sounds, and practice defaults in the previous app; close it, launch Scorewing, and verify state remains available. Check interrupted-export recovery and cross-version lock exclusion with existing recovery tests and an appropriate native smoke check. Do not claim exact platform upgrade behavior from a text-only rename.
- Compare icon image checksums with the baseline; inspect SVG diffs to ensure only incidental metadata changed.
- Audit tracked text and filenames case-insensitively for `voiceweaver`, `voice-weaver`, `voice_weaver`, `mxml-cli`, `mxml_cli`, `mxml-core`, `mxml_core`, `mxml-desktop`, `mxml_desktop`, and broader project-specific `mxml`/`MXML_` occurrences. Every remaining match must belong to the explicit compatibility/format/history exceptions. No old visible app/command/crate branding may remain.
- Confirm dependency versions and user score files have not changed. No new dependencies or test framework are needed.

## Implementation results (2026-10-29)

The Rust, desktop, and tooling/docs subagents completed their assigned changes. The coordinator integrated the result, fixed Rustfmt import ordering/line wrapping, and ran the checks below. An independent read-only review of the combined diff and both moved crate trees reported no findings.

The CLI is now `scorewing`, the desktop executable is `scorewing-desktop`, and branded packages/crate directories/imports follow the naming map. Build scripts, CI artifacts, container/cache names, build variables, command examples, and documentation use Scorewing. The neutral `practice-tracks` crate is unchanged in name. The existing macOS installation test suite now also checks that the first Scorewing installation leaves the former app installation intact.

| Check | Result |
| --- | --- |
| `node --test tools/*.test.mjs` | 13 passed, 1 skipped because PowerShell is unavailable. |
| `npm --prefix apps/desktop test` | All 35 tests passed. |
| `npm --prefix apps/desktop run build` | Passed TypeScript checking and production bundling; HeroUI directive and bundle-size warnings remain. |
| `cargo fmt --all -- --check` with Rust 1.90 | Passed after the integration formatting fix. |
| Locked/offline Cargo metadata without dependencies | Passed; all four packages, moved paths, and renamed executable/library targets resolve. |
| Settings, Tauri identity, and recovery preservation | All three settings modules are byte-identical to the baseline; Tauri configuration changes only product/window display names; recovery/locking source changes only a visible diagnostic name. |
| Icons, schema, and fixtures | All six bundled icon files, SVG artwork, moved schemas, and moved fixtures are byte-identical to the baseline; SVG changes are limited to incidental description metadata. |
| Dependency preservation | Cargo.lock contains only intended local package/dependency-name substitutions; npm lockfile changes only the root package name. Dependency versions/integrity data are unchanged. |
| Legacy-name and diff audit | Remaining old names are documented compatibility, file-format, former-installation test, or historical-plan references. No old branded filenames remain. No introduced whitespace errors, including in the moved/new files. Existing third-party schema formatting was preserved. |
| `cargo test --workspace --locked` | Attempted with Rust 1.90 after successfully fetching locked dependencies. Blocked in `glib-sys` because the host has no `pkg-config` executable. |
| Non-desktop Rust tests for core, CLI, and practice tracks | Also attempted to bypass GTK requirements. Blocked in the `libxml` build script because `pkg-config --static ... libxml-2.0` cannot run on this host. |

Workspace Clippy, executable runtime smoke checks, native package/linkage checks, and packaged upgrade/profile verification remain unverified: the host does not have the project's native build prerequisites prepared. Run the existing check/build scripts in the documented development container and native OS runners before release. Do not disable static-linking requirements or change dependencies to work around this host limitation.

No migration code, command aliases, dependency upgrades, icon redesign, hosted repository changes, publication, installed-app removal, or Git commit was performed. The separate external checklist remains follow-up work.

## Separate external checklist

- Rename the hosted repository from `Popchor-RUB/Voiceweaver` to the chosen Scorewing repository name, if desired. Verify redirects and integrations before updating published links.
- Update local remotes, release upload destinations, CI/release configuration outside tracked files, badges, download links, and any registry entries that actually exist. Preserve authentication securely; do not print or blindly replace credential-bearing remote URLs.
- Check executable/package/app-name collisions and any signing/notarization/release configuration before publication. Keep the stable internal app identifier and signing ownership consistent.
- Keep the current managed checkout path as-is during the work. Rename the local workspace folder separately through the workspace host if desired.
- Document that renamed CLI commands and build environment variables require updates in users' scripts; no compatibility aliases are planned.
- Treat the first `Scorewing.app` installation as a new application path. Preserve rollback for later Scorewing replacements. Leave any old Voiceweaver installation/data in place until the Scorewing installation and saved state are verified; offer manual removal in release instructions rather than automatically deleting it. Do not assume Windows/Linux packages remove older product names without a native installer check.
- Publish a release only after the required native packaging and preservation checks pass. No publishing, hosted repository rename, or installed-app deletion is authorized by this plan alone.

## Risks and stop condition

The fixed internal app identifier is intentional and may remain visible in platform metadata. The retained storage/lock names preserve compatibility without a migration feature. Native installer/profile preservation still needs verification on the exact shipped runtime; it is not proved by retaining the identifier alone. Renamed commands/build variables break callers until those callers are updated, and renamed caches can trigger rebuilds.

Implementation is complete when the naming map, connected callers/tests/build/docs, icon constraints, compatibility exceptions, and applicable validation gates all match this plan. Stop there: no icon redesign, UI redesign, dependency upgrades, settings migrations, automatic old-app removal, or unrelated refactoring.

Suggested implementation commit: `refactor: rename application and crates to Scorewing`
