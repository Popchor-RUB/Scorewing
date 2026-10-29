#!/usr/bin/env bash
set -euo pipefail
node --test tools/*.test.mjs
npm --prefix apps/desktop ci
npm --prefix apps/desktop test
npm --prefix apps/desktop run build
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build -p scorewing-cli --locked
cargo build -p scorewing-cli --release --locked
npm --prefix apps/desktop run tauri -- build --bundles deb,rpm
node tools/check-linkage.mjs "$CARGO_TARGET_DIR/release/scorewing"
node tools/check-linkage.mjs --desktop "$CARGO_TARGET_DIR/release/scorewing-desktop"
