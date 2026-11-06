#!/usr/bin/env node
import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

export const VERSION_FILES = [
  ['Cargo.toml', /(\[workspace\.package\]\r?\nversion = ")([^"]+)(")/g, 1],
  ['Cargo.lock', /(\[\[package\]\]\r?\nname = "(?:practice-tracks|scorewing-cli|scorewing-core|scorewing-desktop)"\r?\nversion = ")([^"]+)(")/g, 4],
  ['apps/desktop/package.json', /("name": "scorewing-desktop",\r?\n\s*"version": ")([^"]+)(")/g, 1],
  ['apps/desktop/package-lock.json', /("name": "scorewing-desktop",\r?\n\s*"version": ")([^"]+)(")/g, 2],
  ['apps/desktop/src-tauri/Cargo.toml', /(\[package\]\r?\nname = "scorewing-desktop"\r?\nversion = ")([^"]+)(")/g, 1],
  ['apps/desktop/src-tauri/tauri.conf.json', /(^  "version": ")([^"]+)(")/gm, 1],
];

const versionPattern = /^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-nightly\.[1-9][0-9]*)?$/;

function prepare(next) {
  if (next !== undefined && !versionPattern.test(next)) {
    throw new Error('Version must be MAJOR.MINOR.PATCH or MAJOR.MINOR.PATCH-nightly.N (N > 0).');
  }
  let current;
  const changes = VERSION_FILES.map(([path, pattern, expected]) => {
    let count = 0;
    const source = readFileSync(path, 'utf8');
    if (path.endsWith('.json')) JSON.parse(source);
    const updated = source.replace(pattern, (_, before, old, after) => {
      if (current === undefined) current = old;
      if (old !== current) throw new Error(`${path}: version ${old} differs from ${current}`);
      count++;
      return `${before}${next ?? old}${after}`;
    });
    if (count !== expected) throw new Error(`${path}: expected ${expected} version entries, found ${count}`);
    if (path.endsWith('.json')) {
      const json = JSON.parse(updated);
      const versions = [json.version];
      if (path.endsWith('package-lock.json')) versions.push(json.packages?.['']?.version);
      if (versions.some(version => version !== (next ?? current))) {
        throw new Error(`${path}: required package version entries are missing or inconsistent`);
      }
    }
    return [path, updated];
  });
  if (!versionPattern.test(current)) throw new Error(`Unsupported repository version: ${current}`);
  return { current, changes };
}

export function readVersion() {
  return prepare().current;
}

export function updateVersion(next) {
  const { changes } = prepare(next);
  for (const [path, content] of changes) writeFileSync(path, content);
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  try {
    if (process.argv.length !== 3) throw new Error('Usage: node tools/update-version.mjs VERSION|--current');
    if (process.argv[2] === '--current') console.log(readVersion());
    else updateVersion(process.argv[2]);
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
