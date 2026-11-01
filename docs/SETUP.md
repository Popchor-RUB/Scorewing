# Development with Podman

The host needs Podman and Git. Rust, Node, Python test tools, libxml2, GTK, WebKitGTK, and packaging tools live in the Ubuntu 24.04 development image. Rust 1.90.0 and Node 22.22.0 are pinned; dependency lockfiles pin application dependencies.

```sh
tools/devcontainer.sh build
tools/devcontainer.sh shell
```

The runner uses Podman's `keep-id` mapping and your host UID/GID as a non-root container user. It binds the repository at `/workspace` and uses named volumes for Cargo downloads and `target/`. Generated source files remain owned by your host account. Build artifacts stay in the `scorewing-target` volume; `tools/package-linux.sh` copies distributables to repository `dist/`.

VS Code Dev Containers can open `.devcontainer/devcontainer.json`. Set its `dev.containers.dockerPath` setting to `podman`. The same Containerfile powers the command-line runner and Linux CI.

## Rename and upgrade notes

Use `scorewing` for the CLI and `scorewing-desktop` for the desktop executable. The Rust packages are `scorewing-cli`, `scorewing-core`, and `scorewing-desktop`; `practice-tracks` keeps its existing name. Automation must use the new commands and build variables; old names have no aliases or fallback handling.

Build variables now use the `SCOREWING_` prefix: `SCOREWING_LOCAL_BUILD`, `SCOREWING_PODMAN_RUNROOT`, `SCOREWING_PODMAN_ROOT`, `SCOREWING_ROOTLESS_UID0`, and `SCOREWING_CACHE_DIR`. Native practice-track checks use `SCOREWING_NATIVE_PROFILE`, as shown in [validation](VALIDATION.md#native-practice-track-audio-checks). The renamed development image and cache volumes may require a fresh build; previous caches and volumes are left untouched.

The internal application identifier remains `org.mxml.desktop`. Existing localStorage keys with the `voiceweaver` prefix, their readers and writers, recovery paths and journals, and `.voiceweaver-metronome.lock` remain unchanged. These internal names preserve saved settings and prevent old and new versions from taking different locks for the same MuseScore preferences. No settings migration is added. MusicXML and the `.mxml` file extension remain supported, and the icon artwork is unchanged.

The first macOS installation creates `/Applications/Scorewing.app`; it leaves any previous differently named app in place. Subsequent Scorewing replacements retain the script's staging, signature verification, backup, and rollback behavior. Before upgrading, close the previous app. Verify recent scores, recent sounds, practice defaults, and interrupted-export recovery in the packaged Scorewing app before manually removing an old installation. Retaining the identifier alone does not establish native profile preservation; verify upgrades on each target platform. Windows and Linux installers also require native checks before assuming they replace previous product names. Existing user data is never removed by this rename.

Hosted repository names, remotes, published download links, and release registries are separate changes; follow the [external checklist](scorewing-rename-plan.md#separate-external-checklist) before publishing.

## Checks and packages

```sh
tools/devcontainer.sh run cargo test --workspace --locked
tools/devcontainer.sh run bash -c 'cd apps/desktop && npm ci && npm test && npm run build'
tools/devcontainer.sh run bash tools/check.sh
tools/devcontainer.sh run bash tools/package-linux.sh
```

The full check includes Rust tests/lints, Python migration regression comparisons, frontend checks, release CLI compilation, and desktop packaging. Python is only a migration test oracle; the shipped app does not invoke it.

For a graphical development session, start the app from a Dev Containers terminal with desktop display forwarding configured for your workstation. Do not expose an unauthenticated X server publicly. Vite listens on port 1420, which the devcontainer forwards.

```sh
cd apps/desktop
npm ci
npm run tauri -- dev
```

Headless Linux startup checks use Xvfb inside the container. Xvfb provides a virtual display, not an interactive host window.

## Restricted runtime directories

If Podman cannot write its normal runtime/storage locations, select writable task-local locations. This does not require installing host dependencies:

```sh
mkdir -p /tmp/scorewing-runtime
chmod 700 /tmp/scorewing-runtime
export XDG_RUNTIME_DIR=/tmp/scorewing-runtime
export SCOREWING_PODMAN_RUNROOT=/tmp/scorewing-runtime/run
export SCOREWING_PODMAN_ROOT=/tmp/scorewing-container-storage
tools/devcontainer.sh build
```

User namespaces and `newuidmap` must be available to Podman. A sandbox that blocks them needs permission to run Podman outside that sandbox; do not use privileged containers as a workaround.

Some older nested rootless runtimes fail with `cannot chown /sys/fs/cgroup/cgroup.procs` when using `keep-id`. For that specific case, set `SCOREWING_ROOTLESS_UID0=1` when invoking the runner. Podman must still run as your unprivileged host account: container UID 0 then maps to that account, so output files retain host ownership. No privileged flag or host root access is used.

## Proxies and certificates

The runner forwards configured proxy variables. If `CODEX_PROXY_CERT` names a CA bundle, it mounts that bundle read-only for runtime tooling and as a build secret. Use a combined system/proxy bundle where required. Credentials and session certificates are not copied into image layers. Dev Containers users with a proxy must supply equivalent build-secret and runtime CA mounts through their local tooling.

## Native platform builds

Local Linux compilation and packaging run in Podman. CI uses one shared workflow on native Ubuntu, Windows, and macOS runners, with pinned Rust and Node versions, for both commit and version-tag pushes. Linux and macOS build checksum-pinned libxml2 2.15.4 from source as a private static library, without optional iconv/ICU dependencies. The local container supplies it under `/opt/libxml2`. Homebrew supplies macOS build tools only. Windows uses `libxml2[core]:x64-windows-static` from vcpkg; Cargo also selects the static MSVC runtime. These CI setup steps do not change the development host.

For a native macOS build, install CMake, pkg-config, LLVM, and Python 3 as build tools, then prepare the static library before running Cargo or Tauri:

```sh
bash tools/build-libxml2.sh "$PWD/target/libxml2"
export PKG_CONFIG_PATH="$PWD/target/libxml2/lib/pkgconfig"
export LIBCLANG_PATH="$(brew --prefix llvm)/lib"
```

Cargo enables static libxml2 linking through `.cargo/config.toml`. A plain system libxml2 installation is not a replacement for the private static build. Release checks inspect both executables and reject non-platform shared libraries. OS frameworks, Windows WebView2, and Linux GTK/WebKitGTK remain platform requirements; Tauri cannot build fully static desktop executables on all operating systems. Trusted macOS CI builds use Developer ID signing and notarization; version-tag builds also sign updater artifacts. See [release credential setup](RELEASING.md). The local macOS script uses ad-hoc signing.

## Local compilation without installing dependencies

Run the appropriate script from any directory:

```sh
# Linux
tools/build-linux.sh
# macOS
tools/build-macos.sh
```

```powershell
# Windows: use an x64 Visual Studio Developer PowerShell
powershell -File tools/build-windows.ps1
```

The scripts check prerequisites before compiling, report what is missing, and exit with a nonzero status on failure. They never run package installers, build/pull container images, run `npm ci`, or fetch Cargo dependencies. Cargo and npm run offline. Prepare dependencies separately before invoking a build script; the following setup commands are intentionally **not** executed by the scripts.

### Linux

The script prefers the existing `localhost/scorewing-dev` Podman image, using the same storage, proxy, and cache settings as `tools/devcontainer.sh`. If Podman or that image is unavailable, it checks the native host instead. Inside a container it builds directly. You can also select native compilation with `SCOREWING_LOCAL_BUILD=1 tools/build-linux.sh`.

Prepare the devcontainer and dependency caches once (requires network access):

```sh
tools/devcontainer.sh build
tools/devcontainer.sh run bash -c 'cd apps/desktop && npm ci'
tools/devcontainer.sh run cargo fetch --locked
```

For a native build, prepare Rust 1.90.0 via rustup, Node.js >=22.22, npm, a C/C++ toolchain, make, pkg-config, binutils (`readelf`), libclang, and development packages for GTK 3, WebKitGTK 4.1, Ayatana AppIndicator 3, and librsvg. Prepare the private static libxml2 with `tools/build-libxml2.sh` and set `PKG_CONFIG_PATH` to its `lib/pkgconfig`. The Containerfile lists the Ubuntu package names. Set `LIBCLANG_PATH` if libclang is outside standard library directories. Separately run `npm --prefix apps/desktop ci` and `cargo fetch --locked` in that prepared environment.

Both the CLI (`dist/scorewing`) and desktop executable (`dist/scorewing-desktop`) are copied to the repository's `dist/`, alongside third-party notices, including when building inside Podman. Linux still needs the GTK/WebKitGTK runtime on the machine running the desktop executable.

### macOS

Use a native Mac; the Linux devcontainer cannot build the Cocoa/WebKit app. Prepare Rust 1.90.0 via rustup, Node.js >=22.22, npm, Xcode Command Line Tools with a macOS SDK, pkg-config, libclang, and the private static libxml2 described above. The script checks `target/libxml2/lib/pkgconfig` automatically, then your existing `PKG_CONFIG_PATH`. It detects Xcode's libclang when available; otherwise set `LIBCLANG_PATH` to LLVM's `lib` directory. Separately run `npm --prefix apps/desktop ci` and `cargo fetch --locked`.

The script builds `target/release/bundle/macos/Scorewing.app`, ad-hoc signs the complete bundle with `codesign --sign -`, verifies it, and stages a verified copy in `/Applications` before replacing `/Applications/Scorewing.app`. It restores the previous app if replacement fails. Quit an existing Scorewing instance before replacing it. Your account must have write access to `/Applications`; the script does not invoke sudo. Ad-hoc signing is for local use and provides no Developer ID or notarization.

The CLI remains at `target/release/scorewing`. `CARGO_TARGET_DIR` may select a different build directory on all platforms.

### Windows

Use native x64 Windows and an x64 Visual Studio Developer PowerShell with C++ Build Tools and a Windows SDK (`cl.exe`, `link.exe`, `rc.exe`). Prepare Rust 1.90.0 with the `x86_64-pc-windows-msvc` host through rustup, Node.js >=22.22, npm, and LLVM under `%ProgramFiles%\LLVM` (including `libclang.dll` and `llvm-readobj.exe`). Set `LIBCLANG_PATH` if libclang is elsewhere.

Prepare `libxml2[core]:x64-windows-static` separately using vcpkg. Set `VCPKG_ROOT` (or `VCPKG_INSTALLATION_ROOT`) to the vcpkg checkout; `VCPKG_INSTALLED_DIR` can select a separate installed tree. The script selects `VCPKGRS_TRIPLET=x64-windows-static`. Separately run `npm --prefix apps/desktop ci` and `cargo fetch --locked`.

Both executables and third-party notices are copied to `dist/`. The desktop executable needs the WebView2 runtime on the machine running it. The scripts skip Windows installers and Linux packages to avoid bundlers downloading WiX, NSIS, WebView2, or other packaging dependencies. The existing CI workflow remains available for distributable installers.
