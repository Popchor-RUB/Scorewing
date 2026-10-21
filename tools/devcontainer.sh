#!/usr/bin/env bash
set -euo pipefail

repo_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
image=localhost/mxml-dev
podman_args=(--cgroup-manager=cgroupfs)
if [[ -n "${MXML_PODMAN_RUNROOT:-}" ]]; then podman_args+=(--runroot "$MXML_PODMAN_RUNROOT"); fi
if [[ -n "${MXML_PODMAN_ROOT:-}" ]]; then podman_args+=(--root "$MXML_PODMAN_ROOT" --storage-driver vfs); fi

case "${1:-shell}" in
  build)
    ca_args=()
    if [[ -n "${CODEX_PROXY_CERT:-}" ]]; then ca_args+=(--secret "id=proxy_ca,src=$CODEX_PROXY_CERT"); fi
    exec podman "${podman_args[@]}" build "${ca_args[@]}" -f "$repo_dir/.devcontainer/Containerfile" -t "$image" "$repo_dir"
    ;;
  shell|run)
    action="${1:-shell}"
    if (($#)); then shift; fi
    if [[ "$action" == shell && $# == 0 ]]; then set -- bash; fi
    if (($# == 0)); then echo 'Usage: tools/devcontainer.sh run COMMAND [ARG...]' >&2; exit 2; fi
    runtime_args=()
    [[ ! -t 0 || ! -t 1 ]] || runtime_args+=(-it)
    for key in HTTP_PROXY HTTPS_PROXY NO_PROXY http_proxy https_proxy no_proxy; do
      if [[ -n "${!key:-}" ]]; then runtime_args+=(-e "$key"); fi
    done
    if [[ -n "${CODEX_PROXY_CERT:-}" ]]; then
      runtime_args+=(--mount "type=bind,src=$CODEX_PROXY_CERT,dst=/run/mxml-proxy-ca.pem,ro=true"
        -e SSL_CERT_FILE=/run/mxml-proxy-ca.pem -e CURL_CA_BUNDLE=/run/mxml-proxy-ca.pem
        -e CARGO_HTTP_CAINFO=/run/mxml-proxy-ca.pem -e NODE_EXTRA_CA_CERTS=/run/mxml-proxy-ca.pem)
    fi
    user_args=(--userns=keep-id --user "$(id -u):$(id -g)")
    # Old nested rootless runtimes cannot chown the host cgroup under keep-id.
    if [[ "${MXML_ROOTLESS_UID0:-0}" == 1 ]]; then user_args=(--user 0:0); fi
    cache_args=(-v mxml-cargo-registry:/opt/cargo/registry:U
      -v mxml-cargo-git:/opt/cargo/git:U -v mxml-target:/workspace/target:U)
    if [[ -n "${MXML_CACHE_DIR:-}" ]]; then
      mkdir -p "$MXML_CACHE_DIR"/{registry,git,target}
      cache_args=(-v "$MXML_CACHE_DIR/registry:/opt/cargo/registry:U"
        -v "$MXML_CACHE_DIR/git:/opt/cargo/git:U" -v "$MXML_CACHE_DIR/target:/workspace/target:U")
    fi
    exec podman "${podman_args[@]}" run --rm --cgroups=disabled --cgroupns=host "${user_args[@]}" -e HOME=/tmp/mxml-home \
      "${runtime_args[@]}" \
      --mount "type=bind,src=$repo_dir,dst=/workspace" \
      "${cache_args[@]}" \
      -w /workspace "$image" "$@"
    ;;
  *) echo 'Usage: tools/devcontainer.sh {build|shell|run COMMAND [ARG...]}' >&2; exit 2 ;;
esac
