import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const tools = dirname(fileURLToPath(import.meta.url));

function fixture(t) {
  const root = mkdtempSync(join(tmpdir(), 'scorewing build '));
  const applications = mkdtempSync(join(tmpdir(), 'scorewing-applications-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  t.after(() => rmSync(applications, { recursive: true, force: true }));
  for (const dir of ['tools', 'bin', 'xml', 'clang', 'docs', 'target/release/bundle/macos/Scorewing.app']) {
    mkdirSync(join(root, dir), { recursive: true });
  }
  for (const name of ['build-linux.sh', 'build-macos.sh', 'devcontainer.sh']) {
    copyFileSync(join(tools, name), join(root, 'tools', name));
  }
  for (const name of ['xml/libxml2.a', 'clang/libclang.so', 'docs/THIRD_PARTY_NOTICES.txt', 'target/release/scorewing', 'target/release/scorewing-desktop']) {
    writeFileSync(join(root, name), 'fixture');
  }
  writeFileSync(join(root, 'clang/libclang.dylib'), 'fixture');
  writeFileSync(join(root, 'target/release/bundle/macos/Scorewing.app/new.txt'), 'new app');
  // Exercise the macOS install flow in an isolated Applications directory.
  const macScript = join(root, 'tools/build-macos.sh');
  writeFileSync(macScript, readFileSync(macScript, 'utf8').replaceAll('/Applications', applications));
  const dispatcher = join(root, 'bin', 'dispatch');
  writeFileSync(dispatcher, `#!/bin/bash
tool="$(/usr/bin/basename "$0")"
printf '%s %s\\n' "$tool" "$*" >> "$TEST_LOG"
case "$tool" in
  uname) echo "$TEST_OS" ;;
  pkg-config)
    [[ "$TEST_MISSING_XML" != 1 ]] || exit 1
    [[ "$1" != --variable=libdir ]] || echo "$TEST_ROOT/xml"
    ;;
  npm) [[ "$TEST_MISSING_NPM" != 1 ]] || exit 1 ;;
  cargo) [[ "$TEST_FAIL_BUILD" != 1 || "$1" != build ]] || exit 9 ;;
  ditto) /bin/cp -R "$1" "$2" ;;
  mv)
    if [[ "$TEST_FAIL_REPLACE" == 1 && "$1" == */.scorewing.*/Scorewing.app ]]; then exit 7; fi
    /bin/mv "$@"
    ;;
esac
`, { mode: 0o755 });
  for (const name of ['uname', 'node', 'npm', 'rustup', 'cargo', 'cc', 'c++', 'make', 'pkg-config', 'readelf', 'podman', 'xcrun', 'codesign', 'ditto', 'otool', 'mv']) {
    symlinkSync(dispatcher, join(root, 'bin', name));
  }
  const env = {
    ...process.env, PATH: `${root}/bin:/usr/bin:/bin`, SCOREWING_LOCAL_BUILD: '1',
    LIBCLANG_PATH: join(root, 'clang'), CARGO_TARGET_DIR: 'target',
    TEST_ROOT: root, TEST_LOG: join(root, 'commands.log'), TEST_OS: 'Linux',
  };
  const run = (script, overrides = {}) => spawnSync('/bin/bash', [join(root, 'tools', script)], {
    cwd: tmpdir(), env: { ...env, ...overrides }, encoding: 'utf8',
  });
  return { root, applications, run, log: () => readFileSync(env.TEST_LOG, 'utf8') };
}

test('Linux builds offline from another directory and copies both binaries', { skip: process.platform !== 'linux' }, (t) => {
  const f = fixture(t);
  const result = f.run('build-linux.sh');
  assert.equal(result.status, 0, result.stderr);
  for (const name of ['scorewing', 'scorewing-desktop', 'THIRD_PARTY_NOTICES.txt']) {
    assert.equal(readFileSync(join(f.root, 'dist', name), 'utf8'), 'fixture');
  }
  assert.match(f.log(), /cargo build -p scorewing-cli --release --locked --offline/);
  assert.match(f.log(), /npm --offline --prefix apps\/desktop run tauri -- build --no-bundle -- --locked --offline/);
  assert.doesNotMatch(f.log(), /(?:^|\s)(ci|install|fetch)(?:\s|$)/m);
});

test('Linux reports missing dependencies together without starting a build', { skip: process.platform !== 'linux' }, (t) => {
  const f = fixture(t);
  const result = f.run('build-linux.sh', { TEST_MISSING_NPM: '1', TEST_MISSING_XML: '1' });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /frontend dependencies/);
  assert.match(result.stderr, /static libxml2/);
  assert.doesNotMatch(f.log(), /cargo (build|metadata)|tauri -- build/);
});

test('failed compilation stops before desktop compilation and publication', { skip: process.platform !== 'linux' }, (t) => {
  const f = fixture(t);
  const result = f.run('build-linux.sh', { TEST_FAIL_BUILD: '1' });
  assert.equal(result.status, 9);
  assert.doesNotMatch(f.log(), /tauri -- build|check-linkage/);
  assert.throws(() => readFileSync(join(f.root, 'dist', 'scorewing')));
});

test('devcontainer readiness only checks the image, using configured storage', { skip: process.platform === 'win32' }, (t) => {
  const f = fixture(t);
  const ready = spawnSync('/bin/bash', [join(f.root, 'tools/devcontainer.sh'), 'ready'], {
    env: { ...process.env, PATH: `${f.root}/bin:/usr/bin:/bin`, TEST_LOG: join(f.root, 'ready.log'), SCOREWING_PODMAN_ROOT: '/tmp/test-storage', SCOREWING_PODMAN_RUNROOT: '/tmp/test-runtime' },
    encoding: 'utf8',
  });
  assert.equal(ready.status, 0, ready.stderr);
  const log = readFileSync(join(f.root, 'ready.log'), 'utf8');
  assert.match(log, /--runroot \/tmp\/test-runtime --root \/tmp\/test-storage --storage-driver vfs image exists localhost\/scorewing-dev/);
  assert.doesNotMatch(log, /\b(build|run|pull)\b/);
});

test('macOS rejects the wrong host without executing build tools', { skip: process.platform !== 'linux' }, (t) => {
  const f = fixture(t);
  const result = f.run('build-macos.sh');
  assert.equal(result.status, 1);
  assert.match(result.stderr, /Run this script on macOS/);
  assert.doesNotMatch(f.log(), /cargo|npm|rustup/);
});

test('macOS signs and verifies the app before replacing the previous installation', { skip: process.platform !== 'linux' }, (t) => {
  const f = fixture(t);
  const installed = join(f.applications, 'Scorewing.app');
  mkdirSync(installed);
  writeFileSync(join(installed, 'old.txt'), 'old app');
  const result = f.run('build-macos.sh', { TEST_OS: 'Darwin' });
  assert.equal(result.status, 0, result.stderr);
  assert.equal(readFileSync(join(installed, 'new.txt'), 'utf8'), 'new app');
  assert.throws(() => readFileSync(join(installed, 'old.txt')));
  const log = f.log();
  assert.match(log, /tauri -- build --bundles app --no-sign -- --locked --offline/);
  assert.ok(log.indexOf('codesign --force --deep --sign -') < log.indexOf('ditto '));
  assert.match(log, /codesign --verify --deep --strict .*\.scorewing\.[^\n]+\n(mv [^\n]+\n){2}/);
  assert.doesNotMatch(log, /(?:^|\s)(ci|install|fetch)(?:\s|$)/m);
});

test('macOS restores the previous installation when replacement fails', { skip: process.platform !== 'linux' }, (t) => {
  const f = fixture(t);
  const installed = join(f.applications, 'Scorewing.app');
  mkdirSync(installed);
  writeFileSync(join(installed, 'old.txt'), 'old app');
  const result = f.run('build-macos.sh', { TEST_OS: 'Darwin', TEST_FAIL_REPLACE: '1' });
  assert.equal(result.status, 7, result.stderr);
  assert.equal(readFileSync(join(installed, 'old.txt'), 'utf8'), 'old app');
  assert.throws(() => readFileSync(join(installed, 'new.txt')));
});

test('macOS leaves the former app installation intact', { skip: process.platform !== 'linux' }, (t) => {
  const f = fixture(t);
  const formerApp = join(f.applications, 'Voiceweaver.app');
  mkdirSync(formerApp);
  writeFileSync(join(formerApp, 'old.txt'), 'old app');
  const result = f.run('build-macos.sh', { TEST_OS: 'Darwin' });
  assert.equal(result.status, 0, result.stderr);
  assert.equal(readFileSync(join(formerApp, 'old.txt'), 'utf8'), 'old app');
  assert.equal(readFileSync(join(f.applications, 'Scorewing.app', 'new.txt'), 'utf8'), 'new app');
});

test('Windows script parses in PowerShell', { skip: process.platform !== 'win32' }, () => {
  const script = join(tools, 'build-windows.ps1').replaceAll("'", "''");
  const result = spawnSync('powershell.exe', ['-NoProfile', '-Command',
    `$tokens=$null; $errors=$null; [System.Management.Automation.Language.Parser]::ParseFile('${script}',[ref]$tokens,[ref]$errors) > $null; if ($errors.Count) { $errors | Out-String | Write-Error; exit 1 }`,
  ], { encoding: 'utf8' });
  assert.equal(result.status, 0, result.stderr);
});
