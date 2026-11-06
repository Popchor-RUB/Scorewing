import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import test from 'node:test';

const files = {
  'Cargo.toml': '[workspace.package]\nversion = "0.2.3"\n',
  'Cargo.lock': ['practice-tracks', 'scorewing-cli', 'scorewing-core', 'scorewing-desktop']
    .map(name => `[[package]]\nname = "${name}"\nversion = "0.2.3"\n`).join('\n'),
  'apps/desktop/package.json': JSON.stringify({ name: 'scorewing-desktop', version: '0.2.3' }, null, 2),
  'apps/desktop/package-lock.json': JSON.stringify({ name: 'scorewing-desktop', version: '0.2.3',
    packages: { '': { name: 'scorewing-desktop', version: '0.2.3' } } }, null, 2),
  'apps/desktop/src-tauri/Cargo.toml': '[package]\nname = "scorewing-desktop"\nversion = "0.2.3"\n',
  'apps/desktop/src-tauri/tauri.conf.json': JSON.stringify({ version: '0.2.3' }, null, 2),
};
const shellOnly = { skip: process.platform === 'win32' };

function fixture(t) {
  const root = mkdtempSync(join(tmpdir(), 'scorewing version '));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const env = { ...process.env, GIT_CONFIG_GLOBAL: '/dev/null', GIT_CONFIG_NOSYSTEM: '1',
    GIT_AUTHOR_NAME: 'Test', GIT_AUTHOR_EMAIL: 'test@example.com',
    GIT_COMMITTER_NAME: 'Test', GIT_COMMITTER_EMAIL: 'test@example.com' };
  function write(path, content) {
    mkdirSync(dirname(join(root, path)), { recursive: true });
    writeFileSync(join(root, path), content);
  }
  for (const [path, content] of Object.entries(files)) write(path, content);
  mkdirSync(join(root, 'tools'));
  for (const name of ['set-version.sh', 'update-version.mjs']) {
    copyFileSync(new URL(name, import.meta.url), join(root, 'tools', name));
  }
  const git = (...args) => execFileSync('git', args, { cwd: root, env, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();
  git('init', '-b', 'main');
  git('add', '.');
  git('commit', '-m', 'initial');
  const run = (args = [], input = '') => spawnSync('bash', ['tools/set-version.sh', ...args], { cwd: root, env, input, encoding: 'utf8' });
  const writer = version => spawnSync(process.execPath, ['tools/update-version.mjs', version], { cwd: root, env, encoding: 'utf8' });
  const snapshot = () => Object.fromEntries(Object.keys(files).map(path => [path, readFileSync(join(root, path), 'utf8')]));
  const versionAt = ref => JSON.parse(git('show', `${ref}:apps/desktop/package.json`)).version;
  const current = () => writer('--current').stdout.trim();
  return { root, git, write, run, writer, snapshot, versionAt, current };
}

test('shared writer synchronizes stable and CI-only nightly versions, including lockfile entries', t => {
  const f = fixture(t);
  for (const version of ['0.2.4', '0.2.4-nightly.17', '0.2.3']) {
    const result = f.writer(version);
    assert.equal(result.status, 0, result.stderr);
    assert.equal(f.current(), version);
    const lock = JSON.parse(readFileSync(join(f.root, 'apps/desktop/package-lock.json'), 'utf8'));
    assert.equal(lock.version, version);
    assert.equal(lock.packages[''].version, version);
    assert.equal((readFileSync(join(f.root, 'Cargo.lock'), 'utf8').match(new RegExp(`version = "${version.replaceAll('.', '\\.')}"`, 'g')) ?? []).length, 4);
  }
});

test('shared writer validates all inputs and manifest agreement before writing any files', t => {
  const f = fixture(t);
  for (const version of ['1.2', 'v1.2.3', '01.2.3', '1.2.3-nightly.0', '1.2.3-nightly.01', '1.2.3-beta.1', '1.2.3+1']) {
    const before = f.snapshot();
    assert.notEqual(f.writer(version).status, 0);
    assert.deepEqual(f.snapshot(), before);
  }
  for (const broken of ['{ malformed', '{\n  "version": "0.2.2"\n}', '{\n  "name": "missing-version"\n}']) {
    f.write('apps/desktop/src-tauri/tauri.conf.json', broken);
    const before = f.snapshot();
    assert.notEqual(f.writer('0.2.4').status, 0);
    assert.deepEqual(f.snapshot(), before);
    assert.notEqual(f.writer('--current').status, 0);
  }
  f.write('apps/desktop/src-tauri/tauri.conf.json', files['apps/desktop/src-tauri/tauri.conf.json']);
  f.write('apps/desktop/package-lock.json', JSON.stringify({ name: 'scorewing-desktop', version: '0.2.3',
    packages: { unrelated: { name: 'scorewing-desktop', version: '0.2.3' } } }, null, 2));
  const before = f.snapshot();
  assert.notEqual(f.writer('0.2.4').status, 0);
  assert.deepEqual(f.snapshot(), before);
});

test('default release tags HEAD and commits the next plain development version', shellOnly, t => {
  const f = fixture(t);
  const release = f.git('rev-parse', 'HEAD');
  const result = f.run([], '\n\n\n');
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /Release version \[0\.2\.3\]/);
  assert.match(result.stdout, /Next development version \[0\.2\.4\]/);
  assert.equal(f.git('rev-parse', 'v0.2.3^{}'), release);
  assert.equal(f.versionAt('v0.2.3'), '0.2.3');
  assert.equal(f.versionAt('HEAD'), '0.2.4');
  assert.equal(f.current(), '0.2.4');
  assert.equal(f.git('rev-list', '--count', 'HEAD'), '2');
  assert.equal(f.git('status', '--porcelain'), '');
  assert.match(result.stdout, /remain local/);
});

test('higher release version gets its own commit and tag before the next-version commit', shellOnly, t => {
  const f = fixture(t);
  const result = f.run(['0.3.0'], '0.4.0\nn\n');
  assert.equal(result.status, 0, result.stderr);
  assert.equal(f.git('rev-parse', 'v0.3.0^{}'), f.git('rev-parse', 'HEAD^'));
  assert.equal(f.versionAt('v0.3.0'), '0.3.0');
  assert.equal(f.versionAt('HEAD'), '0.4.0');
  assert.equal(f.current(), '0.4.0');
  assert.equal(f.git('rev-list', '--count', 'HEAD'), '3');
});

test('invalid release/next, cancellation, and version mismatch leave files, tags, and HEAD unchanged', shellOnly, t => {
  const f = fixture(t);
  const initial = f.git('rev-parse', 'HEAD');
  for (const [args, input] of [
    [[], ''], [[], '\n'], [['0.2.4'], ''], [['0.2.2'], '\n'],
    [['0.2.4'], '0.2.4\n'], [['0.2.4'], '0.2.3\n'], [['0.2.4'], 'garbage\n'],
    [['0.2.4'], '0.2.5-nightly.1\n'], [['0.2.4-nightly.1'], '\n'], [['01.2.3'], '\n'],
  ]) {
    const before = f.snapshot();
    assert.notEqual(f.run(args, input).status, 0);
    assert.deepEqual(f.snapshot(), before);
    assert.equal(f.git('rev-parse', 'HEAD'), initial);
    assert.equal(f.git('tag', '--list'), '');
  }
  f.write('apps/desktop/src-tauri/tauri.conf.json', '{\n  "version": "0.2.2"\n}');
  f.git('add', '.');
  f.git('commit', '-m', 'mismatched manifest');
  assert.notEqual(f.run([], '\n\n').status, 0);
  assert.equal(f.git('tag', '--list'), '');
});

test('dirty tracked files and detached HEAD are refused before changes', shellOnly, t => {
  const f = fixture(t);
  f.write('Cargo.toml', `${files['Cargo.toml']}# dirty\n`);
  assert.match(f.run([], '\n\n').stderr, /Commit or discard/);
  f.git('add', '.');
  assert.match(f.run([], '\n\n').stderr, /Commit or discard/);
  f.git('reset', '--hard', 'HEAD');
  f.git('checkout', '--detach');
  assert.match(f.run([], '\n\n').stderr, /checked-out branch/);
  assert.equal(f.git('tag', '--list'), '');
});

test('release wrapper refuses CI nightly versions in the repository', shellOnly, t => {
  const f = fixture(t);
  assert.equal(f.writer('0.2.4-nightly.1').status, 0);
  f.git('add', '.');
  f.git('commit', '-m', 'CI-only version');
  const before = f.snapshot();
  assert.match(f.run([], '\n\n').stderr, /nightly suffixes belong in CI/);
  assert.deepEqual(f.snapshot(), before);
  assert.equal(f.git('tag', '--list'), '');
});

test('existing local release tags are preserved and explain how to bootstrap', shellOnly, t => {
  const f = fixture(t);
  f.git('tag', 'v0.2.3');
  const before = f.snapshot();
  const result = f.run([], '\n\n');
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /v0\.2\.3 already exists.*next patch/);
  assert.deepEqual(f.snapshot(), before);
  assert.equal(f.git('rev-list', '--count', 'HEAD'), '1');
});

function origin(f, t) {
  const remote = mkdtempSync(join(tmpdir(), 'scorewing remote '));
  t.after(() => rmSync(remote, { recursive: true, force: true }));
  f.git('init', '--bare', remote);
  f.git('remote', 'add', 'origin', remote);
  f.git('push', 'origin', 'main');
  return remote;
}

test('remote-only release tags and an unreachable origin abort before any mutation', shellOnly, t => {
  const f = fixture(t);
  origin(f, t);
  f.git('tag', 'v0.2.3');
  f.git('push', 'origin', 'v0.2.3');
  f.git('tag', '-d', 'v0.2.3');
  const before = f.snapshot();
  assert.match(f.run([], '\n\n').stderr, /v0\.2\.3 already exists/);
  assert.deepEqual(f.snapshot(), before);
  f.git('remote', 'set-url', 'origin', join(f.root, 'missing.git'));
  assert.match(f.run([], '\n\n').stderr, /Could not check origin/);
  assert.deepEqual(f.snapshot(), before);
  assert.equal(f.git('tag', '--list'), '');
});

test('explicit push publishes branch and annotated release tag together', shellOnly, t => {
  const f = fixture(t);
  origin(f, t);
  const result = f.run([], '\n\nyes\n');
  assert.equal(result.status, 0, result.stderr);
  const refs = f.git('ls-remote', 'origin');
  assert.ok(refs.includes(`${f.git('rev-parse', 'HEAD')}\trefs/heads/main`));
  assert.ok(refs.includes(`${f.git('rev-parse', 'v0.2.3^{}')}\trefs/tags/v0.2.3^{}`));
});
