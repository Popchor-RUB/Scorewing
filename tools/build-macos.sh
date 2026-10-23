#!/usr/bin/env bash
set -euo pipefail

repo_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_dir"
[[ "$(uname -s)" == Darwin ]] || { echo 'Run this script on macOS.' >&2; exit 1; }

# The Linux devcontainer cannot compile native Cocoa/WebKit applications.
export PKG_CONFIG_PATH="$repo_dir/target/libxml2/lib/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
missing=()
for tool in node npm rustup cargo pkg-config xcrun codesign ditto otool; do
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
if command -v xcrun >/dev/null; then
  xcrun --show-sdk-path >/dev/null 2>&1 || missing+=('Xcode Command Line Tools and macOS SDK')
  clang="$(xcrun --find clang 2>/dev/null || true)"
  if [[ -z "${LIBCLANG_PATH:-}" && -f "${clang%/bin/clang}/lib/libclang.dylib" ]]; then
    export LIBCLANG_PATH="${clang%/bin/clang}/lib"
  fi
fi
[[ -f "${LIBCLANG_PATH:-/nonexistent}/libclang.dylib" ]] || missing+=('libclang.dylib (set LIBCLANG_PATH, e.g. $(brew --prefix llvm)/lib)')
if command -v pkg-config >/dev/null; then
  xml_libdir="$(pkg-config --variable=libdir libxml-2.0 2>/dev/null || true)"
  [[ -f "$xml_libdir/libxml2.a" ]] || missing+=('private static libxml2 (prepare with bash tools/build-libxml2.sh "$PWD/target/libxml2")')
fi
[[ -w /Applications ]] || missing+=('write access to /Applications (run as a user allowed to install apps)')
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
npm --offline --prefix apps/desktop run tauri -- build --bundles app --no-sign -- --locked --offline
target_dir="$CARGO_TARGET_DIR"
node tools/check-linkage.mjs "$target_dir/release/voiceweaver"
node tools/check-linkage.mjs --desktop "$target_dir/release/voiceweaver-desktop"
app="$target_dir/release/bundle/macos/Voiceweaver.app"
codesign --force --deep --sign - "$app"
codesign --verify --deep --strict "$app"

# Stage the complete copy before replacing an existing installation.
staging="$(mktemp -d /Applications/.voiceweaver.XXXXXX)"
cleanup() {
  if [[ -e "$staging/previous.app" ]]; then
    if [[ ! -e /Applications/Voiceweaver.app && ! -L /Applications/Voiceweaver.app ]]; then
      mv "$staging/previous.app" /Applications/Voiceweaver.app || { echo "Previous app preserved in $staging/previous.app" >&2; return; }
    else
      echo "Previous app preserved in $staging/previous.app" >&2; return
    fi
  fi
  rm -rf "$staging"
}
trap cleanup EXIT
ditto "$app" "$staging/Voiceweaver.app"
codesign --verify --deep --strict "$staging/Voiceweaver.app"
if [[ -e /Applications/Voiceweaver.app || -L /Applications/Voiceweaver.app ]]; then
  mv /Applications/Voiceweaver.app "$staging/previous.app"
fi
mv "$staging/Voiceweaver.app" /Applications/Voiceweaver.app
rm -rf "$staging/previous.app"
echo "Installed ad-hoc signed app at /Applications/Voiceweaver.app; CLI: $target_dir/release/voiceweaver"
