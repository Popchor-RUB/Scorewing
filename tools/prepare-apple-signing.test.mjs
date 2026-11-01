import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const script = fileURLToPath(new URL('./prepare-apple-signing.mjs', import.meta.url));

function fixture(t) {
  const dir = mkdtempSync(join(tmpdir(), 'scorewing-signing-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const env = {
    ...process.env,
    APPLE_CERTIFICATE: 'test-certificate',
    APPLE_CERTIFICATE_PASSWORD: 'test-password',
    APPLE_SIGNING_IDENTITY: 'Developer ID Application: Test (TEAMID)',
    APPLE_API_ISSUER: 'test-issuer',
    APPLE_API_KEY: 'test-key-id',
    APPLE_API_PRIVATE_KEY: 'test-private-key\nsecond-line\n',
    RUNNER_TEMP: dir,
    GITHUB_ENV: join(dir, 'github-env'),
  };
  const keyPath = join(dir, 'scorewing-notarization.p8');
  return { env, keyPath, run: () => spawnSync(process.execPath, [script], { env, encoding: 'utf8' }) };
}

test('prepares a private key file and exports only its path without logging credentials', (t) => {
  const f = fixture(t);
  writeFileSync(f.env.GITHUB_ENV, 'EXISTING=value\n');
  const result = f.run();
  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.stdout + result.stderr, '');
  assert.equal(readFileSync(f.keyPath, 'utf8'), f.env.APPLE_API_PRIVATE_KEY);
  assert.equal(readFileSync(f.env.GITHUB_ENV, 'utf8'), `EXISTING=value\nAPPLE_API_KEY_PATH=${f.keyPath}\n`);
  if (process.platform !== 'win32') assert.equal(statSync(f.keyPath).mode & 0o777, 0o600);
});

test('missing credentials fail before writing a key or exporting a path', (t) => {
  const f = fixture(t);
  for (const name of Object.keys(f.env).filter((name) => name.startsWith('APPLE_'))) {
    const value = f.env[name];
    f.env[name] = '';
    const result = f.run();
    assert.notEqual(result.status, 0);
    assert.ok(result.stderr.includes(`Set ${name} before signing macOS builds`));
    assert.ok(!existsSync(f.keyPath));
    assert.ok(!existsSync(f.env.GITHUB_ENV));
    assert.ok(!result.stderr.includes(f.env.APPLE_API_PRIVATE_KEY || 'second-line'));
    f.env[name] = value;
  }
});

test('rejects a development identity before writing the notarization key', (t) => {
  const f = fixture(t);
  f.env.APPLE_SIGNING_IDENTITY = 'Apple Development: Test';
  const result = f.run();
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /must identify a Developer ID Application certificate/);
  assert.ok(!existsSync(f.keyPath));
});

test('refuses to overwrite an existing key file', (t) => {
  const f = fixture(t);
  writeFileSync(f.keyPath, 'existing-key');
  assert.notEqual(f.run().status, 0);
  assert.equal(readFileSync(f.keyPath, 'utf8'), 'existing-key');
  assert.ok(!existsSync(f.env.GITHUB_ENV));
});
