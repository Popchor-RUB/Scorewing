#!/usr/bin/env bash
set -euo pipefail

if [[ $# -gt 1 ]]; then
  printf 'Usage: %s [MAJOR.MINOR.PATCH]\n' "$0" >&2
  exit 2
fi
cd "$(dirname "$0")/.."

branch=$(git symbolic-ref --quiet --short HEAD) || {
  printf 'A checked-out branch is required.\n' >&2
  exit 1
}
if [[ -n $(git status --porcelain --untracked-files=no) ]]; then
  printf 'Commit or discard existing changes before setting a release version.\n' >&2
  exit 1
fi
current=$(node tools/update-version.mjs --current)
stable_pattern='^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$'
if [[ ! $current =~ $stable_pattern ]]; then
  printf 'Repository version must be plain MAJOR.MINOR.PATCH; nightly suffixes belong in CI.\n' >&2
  exit 1
fi

if [[ $# -eq 1 ]]; then
  release=$1
else
  printf 'Release version [%s]: ' "$current"
  read -r release || { printf '\nCancelled; nothing changed.\n'; exit 1; }
  release=${release:-$current}
fi
if [[ ! $release =~ $stable_pattern ]]; then
  printf 'Release version must be plain MAJOR.MINOR.PATCH.\n' >&2
  exit 2
fi
default_next=$(node -e 'const p = process.argv[1].split("."); p[2] = String(BigInt(p[2]) + 1n); console.log(p.join("."))' "$release")
printf 'Next development version [%s]: ' "$default_next"
read -r next || { printf '\nCancelled; nothing changed.\n'; exit 1; }
next=${next:-$default_next}
if [[ ! $next =~ $stable_pattern ]]; then
  printf 'Next development version must be plain MAJOR.MINOR.PATCH.\n' >&2
  exit 2
fi
node - "$current" "$release" "$next" <<'NODE'
const [current, release, next] = process.argv.slice(2);
function compare(a, b) {
  const left = a.split('.').map(BigInt), right = b.split('.').map(BigInt);
  for (let i = 0; i < 3; i++) {
    if (left[i] !== right[i]) return left[i] > right[i] ? 1 : -1;
  }
  return 0;
}
if (compare(release, current) < 0 || compare(next, release) <= 0) {
  console.error(`Release ${release} must not be below ${current}; next version ${next} must be higher than the release.`);
  process.exit(1);
}
NODE

tag="v$release"
tag_exists() {
  printf 'Tag %s already exists; tags are never replaced. Choose an unused release version (for example, the next patch for an already-tagged checkout).\n' "$tag" >&2
  exit 1
}
if git rev-parse --verify --quiet "refs/tags/$tag" >/dev/null; then tag_exists; fi
if git remote get-url origin >/dev/null 2>&1; then
  remote_tag=$(git ls-remote --tags origin "refs/tags/$tag") || {
    printf 'Could not check origin for tag %s; nothing changed.\n' "$tag" >&2
    exit 1
  }
  if [[ -n $remote_tag ]]; then tag_exists; fi
fi

version_files=(Cargo.toml Cargo.lock apps/desktop/package.json apps/desktop/package-lock.json
  apps/desktop/src-tauri/Cargo.toml apps/desktop/src-tauri/tauri.conf.json)
if [[ $release != "$current" ]]; then
  node tools/update-version.mjs "$release"
  git add -- "${version_files[@]}"
  git commit -m "chore: release $tag"
fi
git tag -a "$tag" -m "Release $tag"

node tools/update-version.mjs "$next"
git add -- "${version_files[@]}"
git commit -m "chore: start development of $next"

printf 'Push %s and tag %s? [y/N] ' "$branch" "$tag"
read -r answer || answer=
case "$answer" in
  y|Y|yes|YES) git push --atomic origin "$branch" "$tag" ;;
  *) printf 'Release tag and next development commit remain local.\n' ;;
esac
