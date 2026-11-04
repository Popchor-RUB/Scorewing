import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const xmlScript = fileURLToPath(new URL('./build-libxml2.sh', import.meta.url));
const workflow = readFileSync(new URL('../.github/workflows/build-platforms.yml', import.meta.url), 'utf8');
const shellOnly = { skip: process.platform === 'win32' };

function fixture(t) {
  const root = mkdtempSync(join(tmpdir(), 'scorewing universal '));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  mkdirSync(join(root, 'bin'));
  mkdirSync(join(root, 'target/release'), { recursive: true });
  const log = join(root, 'commands.jsonl');
  const stub = `#!${process.execPath}
const fs = require('node:fs');
const path = require('node:path');
const tool = path.basename(process.argv[1]);
const args = process.argv.slice(2);
fs.appendFileSync(process.env.TEST_LOG, JSON.stringify({ tool, args }) + '\\n');
if (tool === 'python3' && process.env.TEST_BAD_CHECKSUM) process.exit(1);
if (tool === 'cargo') {
  const target = args[args.indexOf('--target') + 1];
  if (target === process.env.TEST_FAIL_TARGET) process.exit(1);
  const output = path.join('target', target, 'release', 'scorewing');
  fs.mkdirSync(path.dirname(output), { recursive: true });
  fs.writeFileSync(output, target + '\\n');
}
if (tool === 'lipo') {
  const output = args.indexOf('-output');
  fs.writeFileSync(args[output + 1], args.slice(1, output).map(file => fs.readFileSync(file, 'utf8')).join(''));
}
`;
  for (const tool of ['curl', 'python3', 'tar', 'cmake', 'cargo', 'lipo']) {
    writeFileSync(join(root, 'bin', tool), stub, { mode: 0o755 });
  }
  const run = (args, env = {}) => spawnSync('bash', args, {
    cwd: root,
    env: { ...process.env, PATH: `${root}/bin:${process.env.PATH}`, TEST_LOG: log, ...env },
    encoding: 'utf8',
  });
  return { root, run, commands: () => readFileSync(log, 'utf8').trim().split('\n').map(line => JSON.parse(line)) };
}

for (const cmakeArgs of [[], ['-DCMAKE_OSX_ARCHITECTURES=x86_64;arm64']]) {
  test(`static XML build preserves native defaults and CMake arguments: ${cmakeArgs}`, shellOnly, (t) => {
    const f = fixture(t);
    const prefix = join(f.root, 'xml');
    const result = f.run([xmlScript, prefix, ...cmakeArgs]);
    assert.equal(result.status, 0, result.stderr);
    const commands = f.commands();
    assert.deepEqual(commands.map(({ tool }) => tool), ['curl', 'python3', 'tar', 'cmake', 'cmake', 'cmake']);
    const configure = commands[3].args;
    assert.ok(configure.includes(`-DCMAKE_INSTALL_PREFIX=${prefix}`));
    assert.ok(configure.includes('-DBUILD_SHARED_LIBS=OFF'));
    assert.deepEqual(configure.filter(arg => arg.startsWith('-DCMAKE_OSX_ARCHITECTURES=')), cmakeArgs);
    assert.deepEqual(commands[4].args, ['--build', `${prefix}/build`, '--parallel', '2']);
    assert.deepEqual(commands[5].args, ['--install', `${prefix}/build`]);
  });
}

test('static XML checksum failure stops before extraction or compilation', shellOnly, (t) => {
  const f = fixture(t);
  assert.notEqual(f.run([xmlScript, join(f.root, 'xml')], { TEST_BAD_CHECKSUM: '1' }).status, 0);
  assert.deepEqual(f.commands().map(({ tool }) => tool), ['curl', 'python3']);
});

const cliBuild = workflow.match(/- name: Build universal macOS CLI\n[\s\S]*?run: \|\n((?: {10}.*\n)+)/)[1];

test('macOS workflow combines both CLI architectures into the collected binary', shellOnly, (t) => {
  const f = fixture(t);
  const result = f.run(['-e', '-o', 'pipefail', '-c', cliBuild]);
  assert.equal(result.status, 0, result.stderr);
  assert.equal(readFileSync(join(f.root, 'target/release/scorewing'), 'utf8'),
    'x86_64-apple-darwin\naarch64-apple-darwin\n');
  const builds = f.commands().filter(({ tool }) => tool === 'cargo');
  assert.equal(builds.length, 2);
  for (const { args } of builds) {
    assert.deepEqual(args.slice(0, 5), ['build', '-p', 'scorewing-cli', '--release', '--locked']);
  }
});

test('macOS workflow does not combine a partial CLI build', shellOnly, (t) => {
  const f = fixture(t);
  const result = f.run(['-e', '-o', 'pipefail', '-c', cliBuild], { TEST_FAIL_TARGET: 'aarch64-apple-darwin' });
  assert.notEqual(result.status, 0);
  assert.ok(f.commands().every(({ tool }) => tool !== 'lipo'));
});
