#!/usr/bin/env bash
set -euo pipefail
node --test tools/check-linkage.test.mjs
npm --prefix apps/desktop ci
npm --prefix apps/desktop test
npm --prefix apps/desktop run build
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build -p mxml-cli --locked
cargo build -p mxml-core --example transform_fixture --locked
PYTHONPATH=tests/reference python3 -m pytest -q
PYTHONPATH=tests/reference MXML_RUST_ENGINE_BIN="$CARGO_TARGET_DIR/debug/examples/transform_fixture" python3 -m pytest -q -p tests.rust_parity tests/test_split.py tests/test_merge.py
cargo build -p mxml-cli --release --locked
npm --prefix apps/desktop run tauri -- build --bundles deb,rpm
node tools/check-linkage.mjs "$CARGO_TARGET_DIR/release/voiceweaver"
node tools/check-linkage.mjs --desktop "$CARGO_TARGET_DIR/release/voiceweaver-desktop"
