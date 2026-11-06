import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { updateVersion } from './update-version.mjs';

export function nightlyVersion(cwd = process.cwd()) {
  const git = (...args) => execFileSync('git', args, { cwd, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();
  const base = JSON.parse(readFileSync(resolve(cwd, 'apps/desktop/src-tauri/tauri.conf.json'), 'utf8')).version;
  assert.match(base, /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/, 'The repository must contain a plain next-release version');
  const tags = git('tag', '--merged', 'HEAD').split('\n')
    .filter(tag => /^v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(tag));
  const previous = tags.map(tag => ({ tag, count: Number(git('rev-list', '--count', `${tag}..HEAD`)) }))
    .sort((a, b) => a.count - b.count || b.tag.localeCompare(a.tag, 'en', { numeric: true }))[0];
  if (previous) {
    const nextParts = base.split('.').map(BigInt);
    const oldParts = previous.tag.slice(1).split('.').map(BigInt);
    const difference = nextParts.map((part, index) => part - oldParts[index]).find(value => value !== 0n);
    assert.ok(difference > 0n, `Repository version ${base} must exceed ${previous.tag}`);
  }
  const count = previous?.count ?? Number(git('rev-list', '--count', 'HEAD'));
  assert.ok(count > 0, 'A nightly must contain commits after the stable version tag');
  return `${base}-nightly.${count}`;
}

export function nightlyBundleConfig(version, cwd = process.cwd(), platform = process.platform) {
  assert.match(version, /^\d+\.\d+\.\d+-nightly\.[1-9]\d*$/);
  const base = version.split('-')[0];
  const config = {
    bundle: { createUpdaterArtifacts: true },
    plugins: { updater: { endpoints: ['https://github.com/Popchor-RUB/Scorewing/releases/download/nightly/latest.json'] } },
  };
  if (platform === 'darwin') {
    const plist = resolve(cwd, 'apps/desktop/nightly.Info.plist');
    writeFileSync(plist, `<?xml version="1.0" encoding="UTF-8"?>\n<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">\n<plist version="1.0"><dict><key>CFBundleShortVersionString</key><string>${base}</string><key>CFBundleVersion</key><string>${base}</string></dict></plist>\n`);
    config.bundle.macOS = { infoPlist: plist, bundleVersion: base };
  }
  return config;
}

if (import.meta.url === pathToFileURL(resolve(process.argv[1] || '')).href) {
  const version = nightlyVersion();
  updateVersion(version);
  writeFileSync('apps/desktop/nightly.tauri.json', `${JSON.stringify(nightlyBundleConfig(version), null, 2)}\n`);
  console.log(version);
}
