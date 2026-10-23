# Development with Podman

The host needs Podman and Git. Rust, Node, Python test tools, libxml2, GTK, WebKitGTK, and packaging tools live in the Ubuntu 24.04 development image. Rust 1.90.0 and Node 22.22.0 are pinned; dependency lockfiles pin application dependencies.

```sh
tools/devcontainer.sh build
tools/devcontainer.sh shell
```

The runner uses Podman's `keep-id` mapping and your host UID/GID as a non-root container user. It binds the repository at `/workspace` and uses named volumes for Cargo downloads and `target/`. Generated source files remain owned by your host account. Build artifacts stay in the `mxml-target` volume; `tools/package-linux.sh` copies distributables to repository `dist/`.

VS Code Dev Containers can open `.devcontainer/devcontainer.json`. Set its `dev.containers.dockerPath` setting to `podman`. The same Containerfile powers the command-line runner and Linux CI.

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
mkdir -p /tmp/mxml-runtime
chmod 700 /tmp/mxml-runtime
export XDG_RUNTIME_DIR=/tmp/mxml-runtime
export MXML_PODMAN_RUNROOT=/tmp/mxml-runtime/run
export MXML_PODMAN_ROOT=/tmp/mxml-container-storage
tools/devcontainer.sh build
```

User namespaces and `newuidmap` must be available to Podman. A sandbox that blocks them needs permission to run Podman outside that sandbox; do not use privileged containers as a workaround.

Some older nested rootless runtimes fail with `cannot chown /sys/fs/cgroup/cgroup.procs` when using `keep-id`. For that specific case, set `MXML_ROOTLESS_UID0=1` when invoking the runner. Podman must still run as your unprivileged host account: container UID 0 then maps to that account, so output files retain host ownership. No privileged flag or host root access is used.

## Proxies and certificates

The runner forwards configured proxy variables. If `CODEX_PROXY_CERT` names a CA bundle, it mounts that bundle read-only for runtime tooling and as a build secret. Use a combined system/proxy bundle where required. Credentials and session certificates are not copied into image layers. Dev Containers users with a proxy must supply equivalent build-secret and runtime CA mounts through their local tooling.

## Native platform builds

Linux compilation and packaging run in Podman. Windows and macOS need native CI runners for Tauri/WebView platform integration; the workflow pins Rust and Node there too. Linux and macOS build checksum-pinned libxml2 2.15.4 from source as a private static library, without optional iconv/ICU dependencies. The container supplies it under `/opt/libxml2`. Homebrew supplies macOS build tools only. Windows uses `libxml2[core]:x64-windows-static` from vcpkg; Cargo also selects the static MSVC runtime. These CI setup steps do not change the development host.

For a native macOS build, install CMake, pkg-config, LLVM, and Python 3 as build tools, then prepare the static library before running Cargo or Tauri:

```sh
bash tools/build-libxml2.sh "$PWD/target/libxml2"
export PKG_CONFIG_PATH="$PWD/target/libxml2/lib/pkgconfig"
export LIBCLANG_PATH="$(brew --prefix llvm)/lib"
```

Cargo enables static libxml2 linking through `.cargo/config.toml`. A plain system libxml2 installation is not a replacement for the private static build. Release checks inspect both executables and reject non-platform shared libraries. OS frameworks, Windows WebView2, and Linux GTK/WebKitGTK remain platform requirements; Tauri cannot build fully static desktop executables on all operating systems. Developer ID signing, notarization, and automatic updates are outside this release; the local macOS script uses ad-hoc signing.

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

The script prefers the existing `localhost/mxml-dev` Podman image, using the same storage, proxy, and cache settings as `tools/devcontainer.sh`. If Podman or that image is unavailable, it checks the native host instead. Inside a container it builds directly. You can also select native compilation with `MXML_LOCAL_BUILD=1 tools/build-linux.sh`.

Prepare the devcontainer and dependency caches once (requires network access):

```sh
tools/devcontainer.sh build
tools/devcontainer.sh run bash -c 'cd apps/desktop && npm ci'
tools/devcontainer.sh run cargo fetch --locked
```

For a native build, prepare Rust 1.90.0 via rustup, Node.js >=22.22, npm, a C/C++ toolchain, make, pkg-config, binutils (`readelf`), libclang, and development packages for GTK 3, WebKitGTK 4.1, Ayatana AppIndicator 3, and librsvg. Prepare the private static libxml2 with `tools/build-libxml2.sh` and set `PKG_CONFIG_PATH` to its `lib/pkgconfig`. The Containerfile lists the Ubuntu package names. Set `LIBCLANG_PATH` if libclang is outside standard library directories. Separately run `npm --prefix apps/desktop ci` and `cargo fetch --locked` in that prepared environment.

Both the CLI (`dist/voiceweaver`) and desktop executable (`dist/voiceweaver-desktop`) are copied to the repository's `dist/`, alongside third-party notices, including when building inside Podman. Linux still needs the GTK/WebKitGTK runtime on the machine running the desktop executable.

### macOS

Use a native Mac; the Linux devcontainer cannot build the Cocoa/WebKit app. Prepare Rust 1.90.0 via rustup, Node.js >=22.22, npm, Xcode Command Line Tools with a macOS SDK, pkg-config, libclang, and the private static libxml2 described above. The script checks `target/libxml2/lib/pkgconfig` automatically, then your existing `PKG_CONFIG_PATH`. It detects Xcode's libclang when available; otherwise set `LIBCLANG_PATH` to LLVM's `lib` directory. Separately run `npm --prefix apps/desktop ci` and `cargo fetch --locked`.

The script builds `target/release/bundle/macos/Voiceweaver.app`, ad-hoc signs the complete bundle with `codesign --sign -`, verifies it, and stages a verified copy in `/Applications` before replacing `/Applications/Voiceweaver.app`. It restores the previous app if replacement fails. Quit an existing Voiceweaver instance before replacing it. Your account must have write access to `/Applications`; the script does not invoke sudo. Ad-hoc signing is for local use and provides no Developer ID or notarization.

The CLI remains at `target/release/voiceweaver`. `CARGO_TARGET_DIR` may select a different build directory on all platforms.

### Windows

Use native x64 Windows and an x64 Visual Studio Developer PowerShell with C++ Build Tools and a Windows SDK (`cl.exe`, `link.exe`, `rc.exe`). Prepare Rust 1.90.0 with the `x86_64-pc-windows-msvc` host through rustup, Node.js >=22.22, npm, and LLVM under `%ProgramFiles%\LLVM` (including `libclang.dll` and `llvm-readobj.exe`). Set `LIBCLANG_PATH` if libclang is elsewhere.

Prepare `libxml2[core]:x64-windows-static` separately using vcpkg. Set `VCPKG_ROOT` (or `VCPKG_INSTALLATION_ROOT`) to the vcpkg checkout; `VCPKG_INSTALLED_DIR` can select a separate installed tree. The script selects `VCPKGRS_TRIPLET=x64-windows-static`. Separately run `npm --prefix apps/desktop ci` and `cargo fetch --locked`.

Both executables and third-party notices are copied to `dist/`. The desktop executable needs the WebView2 runtime on the machine running it. The scripts skip Windows installers and Linux packages to avoid bundlers downloading WiX, NSIS, WebView2, or other packaging dependencies. The existing CI workflow remains available for distributable installers.
