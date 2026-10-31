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

Updater signatures establish the identity of updates. They do not replace Apple code signing/notarization or Windows Authenticode signing; the existing platform signing behavior remains in place.

## Create a draft

1. Set the same stable version in `apps/desktop/package.json` (and its lockfile), `apps/desktop/src-tauri/Cargo.toml` (and the workspace lockfile), and `apps/desktop/src-tauri/tauri.conf.json`.
2. Run the checks, commit, and push a matching tag such as `v0.1.0` to `Popchor-RUB/Scorewing`.
3. The **Build signed draft release** workflow tests and builds Linux x64, Windows x64, macOS Intel, and macOS Apple Silicon. Platform jobs run sequentially so their updates to `latest.json` cannot overwrite each other.
4. Confirm all four jobs succeeded. Inspect the draft's installers, updater archives, signatures, and `latest.json`. It must include `linux-x86_64`, `windows-x86_64`, `darwin-x86_64`, and `darwin-aarch64` entries, each with a URL and signature. Windows updates use NSIS.
5. Install and smoke-test the platform packages, then publish the draft manually. The workflow never publishes a release and refuses to modify an already-published tag. Failed runs leave a draft that can be retried before publishing.

The workflow enables `bundle.createUpdaterArtifacts` for signed releases. It stays disabled in the base config so normal CI and local builds do not require the production private key. Release downloads include AppImage, DEB/RPM, Windows installers, and macOS DMGs; macOS updates use the signed `.app.tar.gz` archive.

## Verify the first update

Install a baseline signed release, then publish a higher version signed with the same key. On each supported platform, verify startup detection, a manual check, download progress, choosing Later, and installation/restart after closing the score. Ensure an invalid signature is rejected. Installation stays blocked during file operations, exports, and score-pair selection. No scores or editing choices are automatically closed by an update.

Linux in-app installation requires running the AppImage from a writable location. DEB/RPM installs and unpackaged development builds use the manual GitHub download link instead. A read-only installation can download an update but installation reports an error rather than discarding the current session.

The hosted multi-platform workflow and a real version-to-version installation must be checked on their target hosts; frontend mocks and a Rust compile check do not establish those results.
