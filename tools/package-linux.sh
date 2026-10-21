#!/usr/bin/env bash
set -euo pipefail
mkdir -p dist
cp "$CARGO_TARGET_DIR/release/mxml" dist/mxml
artifacts=(dist/mxml)
while IFS= read -r -d '' artifact; do
  destination="dist/$(basename "$artifact")"
  cp "$artifact" "$destination"
  artifacts+=("$destination")
done < <(find "$CARGO_TARGET_DIR/release/bundle" -type f \( -name '*.deb' -o -name '*.rpm' \) -print0)
node tools/checksum.mjs "${artifacts[@]}"
