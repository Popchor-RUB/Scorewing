#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 || ! $1 =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]]; then
  printf 'Usage: %s MAJOR.MINOR.PATCH\n' "$0" >&2
  exit 2
fi

version=$1
tag="v$version"
cd "$(dirname "$0")/.."

branch=$(git symbolic-ref --quiet --short HEAD) || {
  printf 'A checked-out branch is required.\n' >&2
  exit 1
}
status=$(git status --porcelain --untracked-files=no)
if [[ -n $status ]]; then
  printf 'Commit or discard existing changes before setting a release version.\n' >&2
  exit 1
fi
if git rev-parse --verify --quiet "refs/tags/$tag" >/dev/null; then
  printf 'Tag %s already exists.\n' "$tag" >&2
  exit 1
fi

remote=$(git remote get-url origin)
case "$remote" in
  https://github.com/Popchor-RUB/Scorewing|https://github.com/Popchor-RUB/Scorewing.git|git@github.com:Popchor-RUB/Scorewing.git) ;;
  *) printf 'origin must point to Popchor-RUB/Scorewing before releasing.\n' >&2; exit 1 ;;
esac

node - "$version" <<'NODE'
const fs = require('node:fs');
const next = process.argv[2];
const files = [
  ['Cargo.toml', /(\[workspace\.package\]\r?\nversion = ")([^"]+)(")/g, 1],
  ['Cargo.lock', /(\[\[package\]\]\r?\nname = "(?:practice-tracks|scorewing-cli|scorewing-core|scorewing-desktop)"\r?\nversion = ")([^"]+)(")/g, 4],
  ['apps/desktop/package.json', /("name": "scorewing-desktop",\r?\n\s*"version": ")([^"]+)(")/g, 1],
  ['apps/desktop/package-lock.json', /("name": "scorewing-desktop",\r?\n\s*"version": ")([^"]+)(")/g, 2],
  ['apps/desktop/src-tauri/Cargo.toml', /(\[package\]\r?\nname = "scorewing-desktop"\r?\nversion = ")([^"]+)(")/g, 1],
  ['apps/desktop/src-tauri/tauri.conf.json', /(^  "version": ")([^"]+)(")/gm, 1],
];

let current;
const changes = files.map(([path, pattern, expected]) => {
  let count = 0;
  const updated = fs.readFileSync(path, 'utf8').replace(pattern, (_, before, old, after) => {
    if (current === undefined) current = old;
    if (old !== current) throw new Error(`${path}: version ${old} differs from ${current}`);
    count++;
    return `${before}${next}${after}`;
  });
  if (count !== expected) throw new Error(`${path}: expected ${expected} version entries, found ${count}`);
  return [path, updated];
});
const oldParts = current.split('.').map(BigInt);
const newParts = next.split('.').map(BigInt);
const changed = newParts.findIndex((part, index) => part !== oldParts[index]);
if (changed < 0 || newParts[changed] < oldParts[changed]) {
  throw new Error(`${next} must be higher than ${current}`);
}
for (const [path, updated] of changes) fs.writeFileSync(path, updated);

const pkg = JSON.parse(fs.readFileSync('apps/desktop/package.json', 'utf8'));
const lock = JSON.parse(fs.readFileSync('apps/desktop/package-lock.json', 'utf8'));
const tauri = JSON.parse(fs.readFileSync('apps/desktop/src-tauri/tauri.conf.json', 'utf8'));
if ([pkg.version, lock.version, lock.packages[''].version, tauri.version].some(value => value !== next)) {
  throw new Error('Version verification failed');
}
NODE

git add -- Cargo.toml Cargo.lock apps/desktop/package.json apps/desktop/package-lock.json \
  apps/desktop/src-tauri/Cargo.toml apps/desktop/src-tauri/tauri.conf.json
git commit -m "chore: release $tag"
git tag -a "$tag" -m "Release $tag"

printf 'Push %s and tag %s to %s? [y/N] ' "$branch" "$tag" "$remote"
read -r answer || answer=
case "$answer" in
  y|Y|yes|YES) git push --atomic origin "$branch" "$tag" ;;
  *) printf 'Commit and tag remain local.\n' ;;
esac
