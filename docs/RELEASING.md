# Desktop releases and updates

The updater reads the latest **published stable** GitHub release at:

```text
https://github.com/Popchor-RUB/Scorewing/releases/latest/download/latest.json
```

The release page itself is HTML, so it cannot serve as the updater endpoint. Drafts and prereleases do not enter this feed. Until the first release with `latest.json` is published, manual checks report an error; automatic startup checks stay quiet.

Nightly packages use their own updater feed, embedded by CI:

```text
https://github.com/Popchor-RUB/Scorewing/releases/download/nightly/latest.json
```

Every successful push build of the repository's default-branch tip replaces one public **Nightly builds** prerelease. A push containing several commits builds its tip once. Failed or superseded builds leave the last published updater snapshot available. Pull requests, forks, other branches, and manual runs never publish nightlies. The rolling prerelease is marked as a prerelease and never becomes GitHub's latest stable release. Stable installations keep the stable feed; installing a nightly package opts into the nightly feed.

## Signing identity

The public updater key is committed in `apps/desktop/src-tauri/tauri.conf.json`. Keep its matching private key backed up securely outside the repository, for example in a password manager. Losing it prevents updates to already-installed clients; do not regenerate or replace it as part of an ordinary release.

Configure the following repository Actions secrets in **Popchor-RUB/Scorewing**:

- `TAURI_SIGNING_PRIVATE_KEY`: the complete contents of the matching private-key file.
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: only needed if that key is password-protected. The initial key has no password; GitHub encrypts the Actions secret at rest.

With an authenticated GitHub CLI, the key can be uploaded without printing it:

```sh
gh secret set TAURI_SIGNING_PRIVATE_KEY --repo Popchor-RUB/Scorewing < /secure/path/scorewing.key
```

Updater signatures establish the identity of updates. They are separate from Apple code signing/notarization and Windows Authenticode signing.

## Apple signing and notarization

Apple signing and notarization are optional. Branch pushes, version-tag pushes, and manual workflow runs in **Popchor-RUB/Scorewing** use them only when all six Apple secrets below are set. If any are missing, macOS builds continue without Developer ID signing or notarization and skip the signature/ticket checks. Pull-request builds and builds in forks remain unsigned and receive no Apple credentials. Invalid credentials still fail when signing is enabled. Version-tag builds continue to require the separate Tauri updater signing key even when Apple signing is skipped.

You need an Apple Developer Program membership, a **Developer ID Application** certificate with its private key, and an App Store Connect API key with permission to notarize applications. Configure these repository Actions secrets:

| Secret | Value |
| --- | --- |
| `APPLE_CERTIFICATE` | Base64 of the exported Developer ID Application `.p12` certificate and private key. |
| `APPLE_CERTIFICATE_PASSWORD` | Password used when exporting the `.p12` file; use a nonempty password. |
| `APPLE_SIGNING_IDENTITY` | Full identity, such as `Developer ID Application: Your Name (TEAMID)`. |
| `APPLE_API_ISSUER` | Issuer ID from App Store Connect's team API keys page. |
| `APPLE_API_KEY` | API key ID, not the private key contents. |
| `APPLE_API_PRIVATE_KEY` | Complete contents of the downloaded `AuthKey_….p8` file, including PEM headers and newlines. |

Export the certificate and private key together from Keychain Access as a password-protected `.p12`. Convert it without printing it:

```sh
openssl base64 -A -in /secure/path/certificate.p12 -out /secure/path/certificate-base64.txt
gh secret set APPLE_CERTIFICATE --repo Popchor-RUB/Scorewing < /secure/path/certificate-base64.txt
gh secret set APPLE_API_PRIVATE_KEY --repo Popchor-RUB/Scorewing < /secure/path/AuthKey_KEYID.p8
```

The workflow writes the API private key to a temporary file readable only by the runner user and removes it in an always-run cleanup step. Tauri imports the signing certificate, signs with the hardened runtime, submits to Apple's notary service, and staples the ticket during packaging. CI then verifies the app signature and stapled ticket. Apple signing credentials are passed only to the macOS packaging and credential-preparation steps.

When all Apple credentials are configured, every trusted macOS push build contacts Apple, including branch builds. Allow for the notarization service's latency and availability. Builds without Apple credentials can show macOS Gatekeeper warnings when downloaded. Keep local builds ad-hoc signed unless you explicitly configure Apple credentials yourself. See [Tauri's macOS signing instructions](https://v2.tauri.app/distribute/sign/macos/) for obtaining the certificate and API key.

## Shared build pipeline

`.github/workflows/build.yml` handles branch pushes, version-tag pushes (`v[0-9]*`), pull requests, and manual runs. Both ordinary builds and release builds call `.github/workflows/build-platforms.yml` for the same dependency setup, tests, lint, CLI build, desktop packaging, and linkage checks in three jobs: Linux x64, Windows x64, and universal macOS. Linux CI uses the existing native Ubuntu release setup for both kinds of build; the local Podman/devcontainer scripts remain available.

The macOS job builds static libxml2 for both `x86_64` and `arm64`, installs both Rust targets, and packages the desktop with `--target universal-apple-darwin`. It also combines both CLI builds with `lipo`. CI verifies both architecture slices and checks their shared dependencies. Desktop bundles live under `target/universal-apple-darwin/release`; the universal CLI stays at `target/release/scorewing`. Local macOS builds remain native to the host.

Ordinary and nightly builds run platforms in parallel, upload GitHub Actions artifacts, and have read-only repository access. Nightly builds additionally enable updater signatures. Build concurrency is scoped to each run so pushes are not discarded while waiting. Only after all three platform jobs and their checks succeed does one serialized publisher receive `contents: write`. Its `queue: max` concurrency setting queues up to 100 pending publishers. Before any mutation, it verifies the built commit remains in default-branch history and uses GitHub’s compare API to compare it with the source commit in the advertised nightly manifest (falling back to the rolling tag for older manifests): a slow older build cannot supersede a published descendant, while a newer unbuilt or failed commit does not block publication of a successful build. Diverged published history fails explicitly. Version-tag pushes validate matching versions, enable updater signatures, and create or update a **draft** GitHub release with the installers and `latest.json`. The stable release invocation also has `contents: write`; its platforms run sequentially to preserve the updater manifest. A tag push runs this pipeline once, rather than launching separate CI and release builds.

The ordinary Actions downloads contain only the three application package types: Linux `.AppImage`, Windows NSIS `.exe`, and universal macOS `.app.tar.gz`. Each platform uploads its own package. CLI binaries, standalone notices, checksums, and signatures are not included. Nightly builds also upload separate intermediate `nightly-*` artifacts containing the package and signature metadata for the coordinated publisher; these are not the public download links. Ordinary macOS builds use the Tauri action's archive of the built `.app` without requiring the updater signing key.

Other branch pushes, pull requests, and manual runs insert the first seven characters of the built commit hash immediately after the version in Actions filenames, for example `Scorewing_0.2.4-abc1234_linux_x64.AppImage`, `Scorewing_0.2.4-abc1234_windows_x64-setup.exe`, and `Scorewing_0.2.4-abc1234_macos_universal.app.tar.gz`. Pull requests use the checked-out merge commit. These builds leave the app's internal version unchanged; tag-release filenames omit the hash. Nightly filenames use the full nightly version instead.

## Nightly versioning and downloads

The committed repository always contains the next plain stable version, for example `0.2.4`. CI fetches full history, finds the preceding ancestor tag matching exactly `vMAJOR.MINOR.PATCH`, and counts all commits after that tag. The rolling `nightly` tag, prerelease tags, and tags outside the built commit's ancestry are ignored. With 17 commits after `v0.2.3`, CI stamps `0.2.4-nightly.17` into all six version files before dependency installation, tests, and packaging:

- `Cargo.toml` and `Cargo.lock` (the four workspace packages).
- `apps/desktop/package.json` and `apps/desktop/package-lock.json` (including the root package entry).
- `apps/desktop/src-tauri/Cargo.toml` and `apps/desktop/src-tauri/tauri.conf.json`.

If there is no ancestor stable tag, the count includes all commits reachable from the built commit. The base version must exceed the preceding stable tag. Nightly suffixes and the generated Tauri configuration are temporary CI changes and are never committed. For macOS, the generated configuration sets numeric `CFBundleShortVersionString` and `CFBundleVersion` values from the base version before signing while retaining full nightly SemVer for Tauri's runtime updater. Windows NSIS uses Tauri's numeric installer-version mapping.

Download the latest successful nightly from the [rolling nightly release](https://github.com/Popchor-RUB/Scorewing/releases/tag/nightly). Like stable releases, a successfully published nightly contains exactly four uploaded assets: the current Linux x64 AppImage, Windows x64 NSIS `.exe`, universal macOS `.app.tar.gz`, and `latest.json`. Packages retain their full version in the filename, such as `Scorewing_0.2.4-nightly.17_windows_x64-setup.exe`; there are no duplicate fixed-name aliases or retained older snapshots.

`latest.json` uses these versioned names and embedded signatures. Both macOS updater keys share the same universal package and signature. The publisher uploads all three packages first, replaces `latest.json`, moves the `nightly` tag to the built commit, and updates the release notes. Only after publication succeeds does it remove every asset except the current three packages and manifest, including legacy aliases and failed uploads. The manifest includes `source_sha`, which is authoritative for publication ordering even if a later tag or release-metadata API call fails; an older build cannot replace an already advertised newer snapshot. Retry the failed publisher to repair the tag, metadata, or cleanup without changing versioned bytes. Retries accept existing versioned assets only if their bytes match. Failed publication or cleanup can temporarily leave extra assets until a successful retry.

GitHub's asset API replaces `latest.json` by deleting and uploading it, so it can briefly return 404 during replacement. The publisher attempts to restore the previous manifest if upload fails and preserves previous packages until publication succeeds. Cached manifests for older nightlies can then refer to removed downloads; check for updates again to get the current package URLs.

Nightly notes use the same `## Commits` format as stable draft notes: non-merge commit subjects, oldest first, since the latest **published stable GitHub release**, or all reachable history if none exists. This notes boundary intentionally differs from the stable-tag boundary used for the nightly counter. Notes are regenerated on each successful nightly; manually edited stable draft notes remain intact on retries.

New tag drafts list non-merge commit subjects, oldest first, since the latest published stable GitHub release. Drafts and unpublished version tags do not move that boundary. Release builds fetch full Git history; without a previous published release, the notes include all reachable non-merge commits. The version-bump commit is included. Retries preserve the existing draft message, including manual edits, and refuse to modify published releases.

## Create a draft

1. Run the checks with no tracked changes and make sure `origin` points to `Popchor-RUB/Scorewing`.
2. Run `tools/set-version.sh`. It proposes the current plain repository version as the release version, then asks for the next development version (default: the following patch). An optional argument such as `tools/set-version.sh 0.3.0` supplies the release version. The script tags the release commit with an annotated `vMAJOR.MINOR.PATCH` tag, updates all six version files to the next plain development version in a separate commit, and asks before pushing the branch and tag together atomically. If the chosen release version differs from the repository version, it first creates a matching release-version commit. Existing local or remote release tags are never replaced; the next version must be higher than the release. Declining the push leaves the tag and next-version commit locally. No nightly version is written locally.
3. The **Test and build Rust CLI and desktop** workflow's release job tests and builds Linux x64, Windows x64, and a universal macOS app for Intel and Apple Silicon, including macOS code signing and notarization when the Apple credentials are configured. Platform jobs run sequentially so their updates to `latest.json` cannot overwrite each other.
4. Confirm all three jobs succeeded. Inspect the draft's three app downloads and `latest.json`. It must include `linux-x86_64`, `windows-x86_64`, `darwin-x86_64`, and `darwin-aarch64` entries, each with a URL and embedded signature. Both macOS entries must point to the same universal archive with the same signature. Windows updates use NSIS. There should be no standalone `.sig` downloads after a successful run.
5. Install and smoke-test the platform packages, then publish the draft manually. The stable workflow never publishes a release and refuses to modify an already-published stable tag. Failed stable runs leave a draft that can be retried before publishing. The accompanying branch push separately publishes a nightly of the next-development commit after its full matrix succeeds.

The workflow enables `bundle.createUpdaterArtifacts` for stable releases and nightlies. It stays disabled in the base config so other CI and local builds do not require the production private key. Stable releases contain only Linux x64 AppImage, Windows x64 NSIS `.exe`, one universal macOS `.app.tar.gz` archive, and `latest.json`: four uploaded assets. GitHub also displays its two automatic source downloads. On macOS, extract the archive and move `Scorewing.app` to `/Applications`; the same archive works with the updater. DEB, RPM, MSI, and DMG packages are not built in CI, including release runs.

macOS release filenames use `macos_universal`, while the updater platform keys in `latest.json` remain `darwin-x86_64` and `darwin-aarch64`, including for updates from earlier architecture-specific releases.

The release action embeds each updater signature in `latest.json`. After each platform upload, the workflow verifies that the manifest contains signatures and points to existing app downloads, then removes standalone `.sig` assets from the draft. A failed run can leave those temporary signatures behind; retry it before publishing. Existing published releases are left intact.

## Choose an update channel

Check for updates always opens a separate, nonblocking update window. Its Stable/Nightly selector checks the chosen feed immediately and remembers the selection across restarts; the initial selection follows the installed build. Startup checks use the saved channel and stay quiet unless a newer, unskipped update is available. Skipping a version applies to that channel only.

Manual checks can offer an older version when changing channels. Review downgrade opens an explicit confirmation before downloading and installing it; startup never offers a downgrade. Channel switching is disabled while downloading or installing. Installation remains blocked while scores, score selections, or active work are open. Unsupported Linux installations open the selected channel's GitHub downloads instead.

## Verify the first update

Install a baseline signed release, then publish a higher version signed with the same key. On each supported platform, verify startup detection, a manual check, download progress, choosing Later, and installation/restart after closing the score. Ensure an invalid signature is rejected. Installation stays blocked during file operations, exports, and score-pair selection. No scores or editing choices are automatically closed by an update.

Linux in-app installation requires running the AppImage from a writable location. Existing DEB/RPM installs and unpackaged development builds use the manual GitHub download link instead; DEB/RPM users must switch to the AppImage for future releases. Existing Windows MSI users should install the NSIS `.exe` manually; MSI update entries are no longer produced. A read-only installation can download an update but installation reports an error rather than discarding the current session.

The hosted multi-platform workflow and a real version-to-version installation must be checked on their target hosts; frontend mocks and a Rust compile check do not establish those results.
