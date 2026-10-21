#!/usr/bin/env bash
set -euo pipefail
node tools/check-linkage.mjs "$CARGO_TARGET_DIR/release/mxml"
node tools/check-linkage.mjs --desktop "$CARGO_TARGET_DIR/release/mxml-desktop"
mkdir -p dist
cp "$CARGO_TARGET_DIR/release/mxml" dist/mxml
cp docs/THIRD_PARTY_NOTICES.txt dist/
artifacts=(dist/mxml dist/THIRD_PARTY_NOTICES.txt)
while IFS= read -r -d '' artifact; do
  destination="dist/$(basename "$artifact")"
  cp "$artifact" "$destination"
  artifacts+=("$destination")
done < <(find "$CARGO_TARGET_DIR/release/bundle" -type f \( -name '*.deb' -o -name '*.rpm' \) -print0)
node tools/checksum.mjs "${artifacts[@]}"
