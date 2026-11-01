import assert from 'node:assert/strict';
import { appendFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

for (const name of [
  'APPLE_CERTIFICATE',
  'APPLE_CERTIFICATE_PASSWORD',
  'APPLE_SIGNING_IDENTITY',
  'APPLE_API_ISSUER',
  'APPLE_API_KEY',
  'APPLE_API_PRIVATE_KEY',
  'RUNNER_TEMP',
  'GITHUB_ENV',
]) {
  assert.ok(process.env[name], `Set ${name} before signing macOS builds`);
}
assert.match(process.env.APPLE_SIGNING_IDENTITY, /^Developer ID Application:/,
  'APPLE_SIGNING_IDENTITY must identify a Developer ID Application certificate');

const keyPath = join(process.env.RUNNER_TEMP, 'scorewing-notarization.p8');
writeFileSync(keyPath, process.env.APPLE_API_PRIVATE_KEY, { mode: 0o600, flag: 'wx' });
appendFileSync(process.env.GITHUB_ENV, `APPLE_API_KEY_PATH=${keyPath}\n`);
