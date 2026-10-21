#!/usr/bin/env bash
set -euo pipefail

# A private static build avoids Homebrew/system SONAMEs and optional ICU/iconv dependencies.
prefix="${1:?Usage: build-libxml2.sh INSTALL_PREFIX}"
mkdir -p "$prefix"
prefix="$(cd "$prefix" && pwd)"
version=2.15.4
archive="$prefix/libxml2-$version.tar.xz"
curl --fail --location --retry 3 "https://download.gnome.org/sources/libxml2/2.15/libxml2-$version.tar.xz" -o "$archive"
python3 - "$archive" <<'PY'
import hashlib
import pathlib
import sys

expected = "98087fd181d9070724f3fbc65c7377db03038eb92bd882374daff44940138821"
if hashlib.sha256(pathlib.Path(sys.argv[1]).read_bytes()).hexdigest() != expected:
    sys.exit("libxml2 source checksum mismatch")
PY
tar --no-same-owner --no-same-permissions -xf "$archive" -C "$prefix"
cmake -S "$prefix/libxml2-$version" -B "$prefix/build" \
  -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$prefix" -DCMAKE_INSTALL_LIBDIR=lib \
  -DCMAKE_POSITION_INDEPENDENT_CODE=ON -DBUILD_SHARED_LIBS=OFF \
  -DLIBXML2_WITH_ICONV=OFF -DLIBXML2_WITH_ICU=OFF \
  -DLIBXML2_WITH_PROGRAMS=OFF -DLIBXML2_WITH_PYTHON=OFF -DLIBXML2_WITH_TESTS=OFF
cmake --build "$prefix/build" --parallel 2
cmake --install "$prefix/build"
