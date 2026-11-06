import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { copyFileSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

export const NIGHTLY_PACKAGES = {
  Linux: { suffix: 'linux_x64.AppImage', directory: 'appimage', extension: '.AppImage', platforms: ['linux-x86_64'] },
  Windows: { suffix: 'windows_x64-setup.exe', directory: 'nsis', extension: '.exe', platforms: ['windows-x86_64'] },
  macOS: { suffix: 'macos_universal.app.tar.gz', directory: 'macos', extension: '.app.tar.gz', platforms: ['darwin-x86_64', 'darwin-aarch64'] },
};

export function stageNightly({ cwd = process.cwd(), os = process.env.RUNNER_OS, releaseDir = process.env.DESKTOP_RELEASE_DIR } = {}) {
  const packageType = NIGHTLY_PACKAGES[os];
  assert.ok(packageType, 'Unsupported nightly platform');
  const version = JSON.parse(readFileSync(join(cwd, 'apps/desktop/src-tauri/tauri.conf.json'), 'utf8')).version;
  assert.match(version, /^\d+\.\d+\.\d+-nightly\.[1-9]\d*$/);
  const bundleDirectory = join(cwd, releaseDir, 'bundle', packageType.directory);
  const packages = readdirSync(bundleDirectory).filter(name => name.endsWith(packageType.extension));
  assert.equal(packages.length, 1, 'Expected exactly one signed nightly package');
  const signature = readFileSync(join(bundleDirectory, `${packages[0]}.sig`), 'utf8').trim();
  assert.ok(signature, 'Nightly updater signature must not be empty');
  const name = `Scorewing_${version}_${packageType.suffix}`;
  const sha = execFileSync('git', ['rev-parse', 'HEAD'], { cwd, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();
  const output = join(cwd, 'nightly-dist');
  mkdirSync(output, { recursive: true });
  copyFileSync(join(cwd, 'dist', name), join(output, name));
  writeFileSync(join(output, `${os}.json`), `${JSON.stringify({ version, sha, os, name, signature })}\n`);
}

if (import.meta.url === pathToFileURL(resolve(process.argv[1] || '')).href) stageNightly();
