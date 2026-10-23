#!/usr/bin/env bash
set -euo pipefail

repo_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_dir"
[[ "$(uname -s)" == Linux ]] || { echo 'Run this script on Linux.' >&2; exit 1; }

# Reuse a prepared image; building it here would install dependencies.
if [[ "${MXML_LOCAL_BUILD:-0}" != 1 && ! -f /.dockerenv && ! -f /run/.containerenv ]]; then
  if command -v podman >/dev/null && bash tools/devcontainer.sh ready; then
    exec bash tools/devcontainer.sh run env MXML_LOCAL_BUILD=1 bash tools/build-linux.sh
  fi
  echo 'Prepared Podman devcontainer unavailable; checking native dependencies.' >&2
fi

missing=()
for tool in node npm rustup cargo cc c++ make pkg-config readelf; do
  command -v "$tool" >/dev/null || missing+=("$tool")
done
if command -v node >/dev/null; then
  node -e 'const [a,b]=process.versions.node.split(".").map(Number); process.exit(a>22 || (a===22 && b>=22) ? 0 : 1)' || missing+=('Node.js >=22.22')
fi
if command -v rustup >/dev/null; then
  rustup run 1.90.0 rustc --version >/dev/null 2>&1 || missing+=('Rust toolchain 1.90.0 (prepare with rustup toolchain install 1.90.0)')
fi
if command -v npm >/dev/null; then
  npm --prefix apps/desktop ls --depth=0 --offline >/dev/null 2>&1 || missing+=('frontend dependencies (prepare with npm --prefix apps/desktop ci)')
fi
if command -v pkg-config >/dev/null; then
  for library in gtk+-3.0 webkit2gtk-4.1 ayatana-appindicator3-0.1 librsvg-2.0; do
    pkg-config --exists "$library" || missing+=("$library development package")
  done
  xml_libdir="$(pkg-config --variable=libdir libxml-2.0 2>/dev/null || true)"
  [[ -f "$xml_libdir/libxml2.a" ]] || missing+=('static libxml2 (see docs/SETUP.md; set PKG_CONFIG_PATH to its lib/pkgconfig)')
fi
clang_found=0
for library in "${LIBCLANG_PATH:-/nonexistent}"/libclang*.so* /usr/lib/llvm-*/lib/libclang.so* /usr/lib/*-linux-gnu/libclang*.so* /usr/lib/libclang*.so* /usr/lib64/libclang*.so*; do
  if [[ -f "$library" ]]; then clang_found=1; break; fi
done
[[ "$clang_found" == 1 ]] || missing+=('libclang shared library (libclang-dev; or set LIBCLANG_PATH)')
if ((${#missing[@]})); then
  printf 'Missing prerequisites (nothing was installed):\n' >&2
  printf '  - %s\n' "${missing[@]}" >&2
  exit 1
fi

export RUSTUP_TOOLCHAIN=1.90.0 CARGO_NET_OFFLINE=true LIBXML2_STATIC=1
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$repo_dir/target}"
[[ "$CARGO_TARGET_DIR" == /* ]] || export CARGO_TARGET_DIR="$repo_dir/$CARGO_TARGET_DIR"
cargo metadata --locked --offline --format-version 1 >/dev/null || {
  echo 'Prepare the Cargo cache separately with cargo fetch --locked, then retry.' >&2; exit 1;
}
cargo build -p mxml-cli --release --locked --offline
npm --offline --prefix apps/desktop run tauri -- build --no-bundle -- --locked --offline
target_dir="$CARGO_TARGET_DIR"
node tools/check-linkage.mjs "$target_dir/release/voiceweaver"
node tools/check-linkage.mjs --desktop "$target_dir/release/voiceweaver-desktop"
mkdir -p dist
cp "$target_dir/release/voiceweaver" "$target_dir/release/voiceweaver-desktop" docs/THIRD_PARTY_NOTICES.txt dist/
echo "Built CLI and desktop executables in $repo_dir/dist/"
