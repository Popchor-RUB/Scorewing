# Desktop releases and updates

The updater reads the latest **published stable** GitHub release at:

```text
https://github.com/Popchor-RUB/Scorewing/releases/latest/download/latest.json
```

The release page itself is HTML, so it cannot serve as the updater endpoint. Drafts and prereleases do not enter this feed. Until the first release with `latest.json` is published, manual checks report an error; automatic startup checks stay quiet.

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

Both branch pushes and version-tag pushes to **Popchor-RUB/Scorewing** build Developer ID signed, notarized macOS installers. Manual workflow runs in that repository do the same. Pull-request builds and builds in forks remain unsigned and receive no Apple credentials. Missing Apple secrets fail trusted macOS builds rather than silently producing unsigned installers.

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

Every trusted macOS push build contacts Apple, including branch builds. Allow for the notarization service's latency and availability. Keep local builds ad-hoc signed unless you explicitly configure Apple credentials yourself. See [Tauri's macOS signing instructions](https://v2.tauri.app/distribute/sign/macos/) for obtaining the certificate and API key.

## Shared build pipeline

`.github/workflows/build.yml` handles branch pushes, version-tag pushes (`v[0-9]*`), pull requests, and manual runs. Both ordinary builds and release builds call `.github/workflows/build-platforms.yml` for the same dependency setup, tests, lint, CLI build, desktop packaging, linkage checks, and checksum artifacts on all four platforms. Linux CI now uses the existing native Ubuntu release setup for both kinds of build; the local Podman/devcontainer scripts remain available.

Ordinary builds run platforms in parallel, upload GitHub Actions artifacts, and have read-only repository access. Version-tag pushes additionally validate matching versions, enable updater signatures, and create or update a **draft** GitHub release with the installers and `latest.json`. Only the release invocation has `contents: write`; its platforms run sequentially to preserve the updater manifest. A tag push runs this pipeline once, rather than launching separate CI and release builds.

## Create a draft

1. Set the same stable version in `apps/desktop/package.json` (and its lockfile), `apps/desktop/src-tauri/Cargo.toml` (and the workspace lockfile), and `apps/desktop/src-tauri/tauri.conf.json`.
2. Run the checks, commit, and push a matching tag such as `v0.1.0` to `Popchor-RUB/Scorewing`.
3. The **Test and build Rust CLI and desktop** workflow's release job tests and builds Linux x64, Windows x64, macOS Intel, and macOS Apple Silicon, including macOS code signing and notarization. Platform jobs run sequentially so their updates to `latest.json` cannot overwrite each other.
4. Confirm all four jobs succeeded. Inspect the draft's installers, updater archives, signatures, and `latest.json`. It must include `linux-x86_64`, `windows-x86_64`, `darwin-x86_64`, and `darwin-aarch64` entries, each with a URL and signature. Windows updates use NSIS.
5. Install and smoke-test the platform packages, then publish the draft manually. The workflow never publishes a release and refuses to modify an already-published tag. Failed runs leave a draft that can be retried before publishing.

The workflow enables `bundle.createUpdaterArtifacts` for signed releases. It stays disabled in the base config so normal CI and local builds do not require the production private key. Release downloads include AppImage, DEB/RPM, Windows installers, and macOS DMGs; macOS updates use the signed `.app.tar.gz` archive.

## Verify the first update

Install a baseline signed release, then publish a higher version signed with the same key. On each supported platform, verify startup detection, a manual check, download progress, choosing Later, and installation/restart after closing the score. Ensure an invalid signature is rejected. Installation stays blocked during file operations, exports, and score-pair selection. No scores or editing choices are automatically closed by an update.

Linux in-app installation requires running the AppImage from a writable location. DEB/RPM installs and unpackaged development builds use the manual GitHub download link instead. A read-only installation can download an update but installation reports an error rather than discarding the current session.

The hosted multi-platform workflow and a real version-to-version installation must be checked on their target hosts; frontend mocks and a Rust compile check do not establish those results.
