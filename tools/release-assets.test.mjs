import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import test from 'node:test';
import { releaseNotes } from './release-notes.mjs';

const workflow = readFileSync(new URL('../.github/workflows/build-platforms.yml', import.meta.url), 'utf8');
const source = workflow.match(/- name: Remove standalone updater signatures[\s\S]*?script: \|\n((?: {12}.*\n)+)/)[1];
const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;
const cleanup = new AsyncFunction('require', 'github', 'context', 'process', source);

function fixture() {
  const names = ['Scorewing_linux.AppImage', 'Scorewing_windows-setup.exe',
    'Scorewing_macos_universal.app.tar.gz'];
  const assets = ['latest.json', ...names, ...names.map(name => `${name}.sig`)].map((name, id) => ({
    id, name, browser_download_url: `https://github.com/Popchor-RUB/Scorewing/releases/download/v1.0.0/${name}`,
  }));
  const platforms = ['linux-x86_64', 'windows-x86_64', 'darwin-x86_64', 'darwin-aarch64'];
  const manifest = { platforms: Object.fromEntries(platforms.map((platform, i) => {
    const asset = assets[Math.min(i, 2) + 1];
    return [platform, { url: asset.browser_download_url, signature: `signature-${asset.id}` }];
  })) };
  const release = { draft: true, tag_name: 'v1.0.0' };
  const deleted = [];
  const github = {
    rest: { repos: {
      getRelease: async (params) => {
        assert.deepEqual(params, { owner: 'Popchor-RUB', repo: 'Scorewing', release_id: 42 });
        return { data: release };
      },
      listReleaseAssets: () => {},
      getReleaseAsset: async (params) => {
        assert.equal(params.asset_id, 0);
        assert.equal(params.headers.accept, 'application/octet-stream');
        return { data: Uint8Array.from(Buffer.from(JSON.stringify(manifest))).buffer };
      },
      deleteReleaseAsset: async ({ asset_id }) => deleted.push(asset_id),
    } },
    paginate: async () => assets,
  };
  return { assets, manifest, release, deleted, run: () => cleanup(
    name => { assert.equal(name, 'node:assert/strict'); return assert; }, github,
    { repo: { owner: 'Popchor-RUB', repo: 'Scorewing' } }, { env: { RELEASE_ID: '42' } },
  ) };
}

test('removes only standalone signatures while preserving the shared universal macOS archive', async () => {
  const f = fixture();
  await f.run();
  assert.deepEqual(f.deleted, f.assets.filter(asset => asset.name.endsWith('.sig')).map(asset => asset.id));
  assert.deepEqual(f.manifest.platforms['darwin-x86_64'], f.manifest.platforms['darwin-aarch64']);
});

test('matches final updater URLs to temporary GitHub draft asset URLs', async () => {
  const f = fixture();
  for (const asset of f.assets) {
    asset.browser_download_url = asset.browser_download_url.replace('/download/v1.0.0/', '/download/untagged-deadbeef/');
  }
  await f.run();
  assert.deepEqual(f.deleted, f.assets.filter(asset => asset.name.endsWith('.sig')).map(asset => asset.id));
});

test('preserves all assets when the manifest or an updater signature or download is missing', async () => {
  const invalid = [
    f => { f.assets.shift(); },
    f => { f.manifest.platforms = {}; },
    f => { Object.values(f.manifest.platforms)[0].signature = ''; },
    f => { f.assets.splice(1, 1); },
    f => { Object.values(f.manifest.platforms)[0].url = 'https://example.com/missing.AppImage'; },
    f => { Object.values(f.manifest.platforms)[0].url = f.assets[1].browser_download_url.replace('/v1.0.0/', '/v0.9.0/'); },
    f => { Object.values(f.manifest.platforms)[0].url = f.assets[1].browser_download_url.replace('/Popchor-RUB/', '/other/'); },
    f => { Object.values(f.manifest.platforms)[0].url = f.assets.at(-1).browser_download_url; },
  ];
  for (const change of invalid) {
    const f = fixture();
    change(f);
    await assert.rejects(f.run());
    assert.deepEqual(f.deleted, []);
  }
});

test('refuses to change a published release', async () => {
  const f = fixture();
  f.release.draft = false;
  await assert.rejects(f.run(), /Only draft release assets may be changed/);
  assert.deepEqual(f.deleted, []);
});

const collectSource = workflow.match(/- name: Collect app packages[\s\S]*?run: \|\n((?: {10}.*\n)+)/)[1];
const shellOnly = { skip: process.platform === 'win32' };

function buildFixture(t) {
  const root = mkdtempSync(join(tmpdir(), 'scorewing ci '));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  let clock = 0;
  const git = (...args) => execFileSync('git', args, {
    cwd: root, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'],
    env: { ...process.env, GIT_AUTHOR_NAME: 'Test', GIT_AUTHOR_EMAIL: 'test@example.com',
      GIT_COMMITTER_NAME: 'Test', GIT_COMMITTER_EMAIL: 'test@example.com',
      GIT_AUTHOR_DATE: `2026-01-01T00:00:${String(clock).padStart(2, '0')}Z`,
      GIT_COMMITTER_DATE: `2026-01-01T00:00:${String(clock++).padStart(2, '0')}Z` },
  }).trim();
  git('init', '-b', 'main');
  git('commit', '--allow-empty', '-m', 'initial');
  git('tag', 'v0.1.0');
  git('checkout', '-b', 'feature');
  git('commit', '--allow-empty', '-m', 'feat: duet', '-m', 'Body must not appear in the notes.');
  git('checkout', 'main');
  git('commit', '--allow-empty', '-m', 'fix: tempo');
  git('merge', '--no-ff', 'feature', '-m', 'Merge feature');
  git('tag', 'v0.2.0');
  git('commit', '--allow-empty', '-m', 'chore: release v0.2.3');
  const write = (path, content = 'package') => {
    mkdirSync(dirname(join(root, path)), { recursive: true });
    writeFileSync(join(root, path), content);
  };
  write('apps/desktop/src-tauri/tauri.conf.json', '{"version":"0.2.3"}');
  return { root, git, write };
}

function notesFixture(t) {
  const f = buildFixture(t);
  const releases = [{ draft: true, tag_name: 'v0.2.0', body: 'Unpublished draft' }];
  let latest = { tag_name: 'v0.1.0' };
  let latestError;
  const github = {
    paginate: async () => releases,
    rest: { repos: {
      listReleases: () => {},
      getLatestRelease: async () => {
        if (latestError) throw latestError;
        return { data: latest };
      },
    } },
  };
  return { ...f, releases,
    setLatest: value => { latest = value; },
    failLatest: error => { latestError = error; },
    run: (nightly = false) => releaseNotes({ github, repo: { owner: 'Popchor-RUB', repo: 'Scorewing' },
      tag: 'v0.2.3', nightly, cwd: f.root }),
  };
}

test('draft notes use the latest published release, chronological subjects, and no merges or bodies', async (t) => {
  const f = notesFixture(t);
  const body = await f.run();
  assert.ok(body.endsWith('## Commits\n\n- feat: duet\n- fix: tempo\n- chore: release v0.2.3\n'), body);
});

test('first release notes include all non-merge history', async (t) => {
  const f = notesFixture(t);
  f.failLatest({ status: 404 });
  assert.ok((await f.run()).endsWith('- initial\n- feat: duet\n- fix: tempo\n- chore: release v0.2.3\n'));
});

test('nightly notes use the same chronological subjects and regenerate after stable drafts', async (t) => {
  const f = notesFixture(t);
  f.releases.push({ tag_name: 'v0.2.3', draft: true, body: 'Manual stable draft' });
  const first = await f.run(true);
  assert.ok(first.startsWith('Latest successful default-branch build.'));
  assert.ok(first.endsWith('## Commits\n\n- feat: duet\n- fix: tempo\n- chore: release v0.2.3\n'));
  f.git('commit', '--allow-empty', '-m', 'fix: nightly followup', '-m', 'Do not show this body');
  assert.ok((await f.run(true)).endsWith('- fix: nightly followup\n'));
});

test('draft retries preserve manual text and published releases are refused', async (t) => {
  const f = notesFixture(t);
  const existing = { tag_name: 'v0.2.3', draft: true, body: 'Manually edited\n\nRelease notes' };
  f.releases.push(existing);
  f.failLatest(new Error('Must not regenerate notes on retry'));
  assert.equal(await f.run(), existing.body);
  existing.draft = false;
  await assert.rejects(f.run(), /already published/);
});

test('release notes fail on API errors or missing baseline tags rather than using all history', async (t) => {
  const f = notesFixture(t);
  f.setLatest({ tag_name: 'v9.9.9' });
  await assert.rejects(f.run(), /git log/);
  f.failLatest({ status: 403 });
  await assert.rejects(f.run(), error => error.status === 403);
});

for (const [os, directory, filename, suffix] of [
  ['Linux', 'appimage', 'Scorewing_0.2.3_amd64.AppImage', 'linux_x64.AppImage'],
  ['Windows', 'nsis', 'Scorewing_0.2.3_x64-setup.exe', 'windows_x64-setup.exe'],
  ['macOS', 'macos', 'Scorewing.app.tar.gz', 'macos_universal.app.tar.gz'],
]) {
  test(`${os} CI collects only its release package type with a hash after the unchanged version`, shellOnly, (t) => {
    const f = buildFixture(t);
    const releaseDir = os === 'macOS' ? 'target/universal-apple-darwin/release' : 'target/release';
    const bundleDir = `${releaseDir}/bundle/${directory}`;
    f.write(`${bundleDir}/${filename}`, 'original package');
    for (const name of [`${filename}.sig`, 'extra.msi', 'extra.deb', 'extra.rpm', 'extra.dmg', 'latest.json']) {
      f.write(`${bundleDir}/${name}`);
    }
    f.write('target/release/scorewing');
    f.write('docs/THIRD_PARTY_NOTICES.txt');
    const run = release => spawnSync('bash', ['-e', '-o', 'pipefail', '-c', collectSource], {
      cwd: f.root, encoding: 'utf8',
      env: { ...process.env, RUNNER_OS: os, DESKTOP_RELEASE_DIR: releaseDir, IS_RELEASE: String(release) },
    });
    for (const release of [false, true]) {
      const result = run(release);
      assert.equal(result.status, 0, result.stderr);
      const version = `0.2.3${release ? '' : `-${f.git('rev-parse', 'HEAD').slice(0, 7)}`}`;
      const expected = `Scorewing_${version}_${suffix}`;
      assert.deepEqual(readdirSync(join(f.root, 'dist')), [expected]);
      assert.equal(readFileSync(join(f.root, 'dist', expected), 'utf8'), 'original package');
      assert.equal(readFileSync(join(f.root, 'apps/desktop/src-tauri/tauri.conf.json'), 'utf8'), '{"version":"0.2.3"}');
      rmSync(join(f.root, 'dist'), { recursive: true });
    }
    rmSync(join(f.root, bundleDir, filename));
    assert.notEqual(run(false).status, 0, 'A missing app package must fail the build');
    f.write(`${bundleDir}/${filename}`);
    f.write(`${bundleDir}/duplicate-${filename}`);
    assert.notEqual(run(false).status, 0, 'Ambiguous app packages must fail the build');
  });
}

test('all runs select the release bundle types and tag builds fetch history for the draft body', () => {
  assert.match(workflow, /--bundles \$\{\{ matrix.bundle \}\}/);
  assert.match(workflow, /fetch-depth: \$\{\{ \(inputs.release \|\| inputs.nightly\) && '0' \|\| '1' \}\}/);
  assert.match(workflow, /releaseBody: \$\{\{ steps.release-notes.outputs.result \}\}/);
});
