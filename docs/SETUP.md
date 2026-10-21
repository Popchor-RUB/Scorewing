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

Linux compilation and packaging run in Podman. Windows and macOS need native CI runners for Tauri/WebView platform integration; the workflow pins Rust and Node there too. Windows uses vcpkg libxml2; macOS uses Homebrew libxml2. These CI setup steps do not change the development host. Signing, notarization, and automatic updates are outside this release.
