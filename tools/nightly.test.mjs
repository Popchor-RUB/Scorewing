import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { nightlyBundleConfig, nightlyVersion } from './nightly-version.mjs';
import { publishNightly } from './publish-nightly.mjs';
import { NIGHTLY_PACKAGES, stageNightly } from './stage-nightly.mjs';

function fixture(t) {
  const root = mkdtempSync(join(tmpdir(), 'scorewing nightly '));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const git = (...args) => execFileSync('git', args, { cwd: root, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'],
    env: { ...process.env, GIT_AUTHOR_NAME: 'Test', GIT_AUTHOR_EMAIL: 'test@example.com',
      GIT_COMMITTER_NAME: 'Test', GIT_COMMITTER_EMAIL: 'test@example.com' } }).trim();
  git('init', '-b', 'main');
  git('commit', '--allow-empty', '-m', 'initial');
  const write = (path, data) => {
    const parts = path.split('/');
    parts.pop();
    mkdirSync(join(root, ...parts), { recursive: true });
    writeFileSync(join(root, path), data);
  };
  write('apps/desktop/src-tauri/tauri.conf.json', '{"version":"0.2.4"}');
  return { root, git, write };
}

test('nightly versions count ancestor commits, ignore rolling/malformed/unmerged tags, and require next base', (t) => {
  const f = fixture(t);
  assert.equal(nightlyVersion(f.root), '0.2.4-nightly.1');
  f.git('tag', 'v0.2.3');
  f.git('tag', 'v0.1.0');
  f.git('checkout', '-b', 'other');
  f.git('commit', '--allow-empty', '-m', 'unmerged');
  f.git('tag', 'v9.0.0');
  f.git('checkout', 'main');
  f.git('commit', '--allow-empty', '-m', 'next base');
  f.git('tag', 'nightly');
  f.git('tag', 'v99.0.0-beta.1');
  f.git('tag', 'v099.0.0');
  assert.equal(nightlyVersion(f.root), '0.2.4-nightly.1');
  f.git('commit', '--allow-empty', '-m', 'feature');
  assert.equal(nightlyVersion(f.root), '0.2.4-nightly.2');
  f.write('apps/desktop/src-tauri/tauri.conf.json', '{"version":"0.2.3"}');
  assert.throws(() => nightlyVersion(f.root), /must exceed/);
  f.write('apps/desktop/src-tauri/tauri.conf.json', '{"version":"0.2.4-nightly.2"}');
  assert.throws(() => nightlyVersion(f.root), /plain next-release/);
});

test('CI entry point stamps all six version files and writes only a temporary nightly configuration', (t) => {
  const f = fixture(t);
  for (const path of ['Cargo.toml', 'Cargo.lock', 'apps/desktop/package.json', 'apps/desktop/package-lock.json',
    'apps/desktop/src-tauri/Cargo.toml', 'apps/desktop/src-tauri/tauri.conf.json']) {
    f.write(path, readFileSync(new URL(`../${path}`, import.meta.url)));
  }
  mkdirSync(join(f.root, 'tools'));
  copyFileSync(new URL('update-version.mjs', import.meta.url), join(f.root, 'tools/update-version.mjs'));
  const writer = args => spawnSync(process.execPath, ['tools/update-version.mjs', ...args], {
    cwd: f.root, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'],
  });
  assert.equal(writer(['0.2.4']).status, 0);
  f.git('tag', 'v0.2.3');
  f.git('add', '.');
  f.git('commit', '-m', 'start next version');
  const head = f.git('rev-parse', 'HEAD');
  const result = spawnSync(process.execPath, [fileURLToPath(new URL('nightly-version.mjs', import.meta.url))], {
    cwd: f.root, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'],
  });
  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.stdout.trim(), '0.2.4-nightly.1');
  assert.equal(writer(['--current']).stdout.trim(), '0.2.4-nightly.1');
  assert.equal(f.git('rev-parse', 'HEAD'), head, 'CI must not commit nightly suffixes');
  assert.equal(JSON.parse(readFileSync(join(f.root, 'apps/desktop/nightly.tauri.json'))).bundle.createUpdaterArtifacts, true);
});

test('nightly bundle override keeps runtime SemVer and fixes Apple numeric metadata before signing', (t) => {
  const f = fixture(t);
  const config = nightlyBundleConfig('0.2.4-nightly.17', f.root, 'darwin');
  assert.equal(config.version, undefined);
  assert.equal(config.bundle.macOS.bundleVersion, '0.2.4');
  const plist = readFileSync(config.bundle.macOS.infoPlist, 'utf8');
  assert.match(plist, /CFBundleShortVersionString<\/key><string>0\.2\.4<\/string>/);
  assert.match(plist, /CFBundleVersion<\/key><string>0\.2\.4<\/string>/);
  assert.ok(!plist.includes('nightly'));
  assert.equal(config.bundle.createUpdaterArtifacts, true);
  assert.deepEqual(config.plugins.updater.endpoints, ['https://github.com/Popchor-RUB/Scorewing/releases/download/nightly/latest.json']);
  assert.equal(nightlyBundleConfig('0.2.4-nightly.17', f.root, 'win32').bundle.macOS, undefined);
});

test('nightly staging requires one package and signature and preserves universal platform metadata', (t) => {
  const f = fixture(t);
  f.write('apps/desktop/src-tauri/tauri.conf.json', '{"version":"0.2.4-nightly.1"}');
  f.write('target/release/bundle/macos/Scorewing.app.tar.gz', 'archive');
  f.write('dist/Scorewing_0.2.4-nightly.1_macos_universal.app.tar.gz', 'archive');
  const run = () => stageNightly({ cwd: f.root, os: 'macOS', releaseDir: 'target/release' });
  assert.throws(run, /ENOENT/);
  f.write('target/release/bundle/macos/Scorewing.app.tar.gz.sig', 'signature\n');
  run();
  const metadata = JSON.parse(readFileSync(join(f.root, 'nightly-dist/macOS.json'), 'utf8'));
  assert.equal(metadata.signature, 'signature');
  assert.equal(metadata.sha, f.git('rev-parse', 'HEAD'));
  assert.equal(readFileSync(join(f.root, 'nightly-dist', metadata.name), 'utf8'), 'archive');
  f.write('target/release/bundle/macos/duplicate.app.tar.gz', 'extra');
  assert.throws(run, /exactly one/);
});

function publisherFixture(t) {
  const f = fixture(t);
  const sha = f.git('rev-parse', 'HEAD');
  const context = { repo: { owner: 'Popchor-RUB', repo: 'Scorewing' }, eventName: 'push',
    ref: 'refs/heads/main', sha, payload: { repository: { default_branch: 'main' } } };
  const version = '0.2.4-nightly.1';
  for (const [os, type] of Object.entries(NIGHTLY_PACKAGES)) {
    const name = `Scorewing_${version}_${type.suffix}`;
    f.write(`stage/${os}.json`, JSON.stringify({ os, name, version, sha, signature: `signature-${os}` }));
    f.write(`stage/${name}`, `new-${os}`);
  }
  let id = 0;
  const assets = [];
  const changes = [];
  const releases = [{ id: 42, tag_name: 'nightly', draft: false, prerelease: true, upload_url: 'https://upload.test' }];
  const addAsset = (name, data, extra = {}) => {
    const buffer = Buffer.from(data);
    const asset = { id: ++id, name, data: buffer, created_at: `2026-01-${String(Math.min(id, 28)).padStart(2, '0')}T00:00:00Z`,
      digest: `sha256:${createHash('sha256').update(buffer).digest('hex')}`, ...extra };
    assets.push(asset);
    return asset;
  };
  addAsset('latest.json', '{"version":"old","platforms":{}}');
  for (const type of Object.values(NIGHTLY_PACKAGES)) addAsset(`Scorewing_${type.suffix}`, 'old-alias');
  let head = sha;
  let tagSha = sha;
  let remoteHead;
  let uploadFailure;
  let tagFailure = false;
  let tagExists = true;
  const github = {
    paginate: async (method) => method === github.rest.repos.listReleases ? releases : assets.slice(),
    rest: { repos: {
      listReleases() {}, listReleaseAssets() {},
      getLatestRelease: async () => { throw { status: 404 }; },
      getBranch: async () => ({ data: { commit: { sha: head } } }),
      compareCommits: async ({ base, head: comparedHead }) => {
        if (base === comparedHead) return { data: { status: 'identical' } };
        if (base === sha && comparedHead === remoteHead) return { data: { status: 'ahead' } };
        if (base === remoteHead && comparedHead === sha) return { data: { status: 'behind' } };
        const ancestor = (a, b) => {
          try { f.git('merge-base', '--is-ancestor', a, b); return true; }
          catch (error) { if (error.status === 1) return false; throw error; }
        };
        return { data: { status: ancestor(base, comparedHead) ? 'ahead' : ancestor(comparedHead, base) ? 'behind' : 'diverged' } };
      },
      createRelease: async params => {
        changes.push(['createRelease', params]);
        const release = { ...params, id: 43, upload_url: 'https://upload.test' };
        releases.push(release);
        return { data: release };
      },
      getReleaseAsset: async ({ asset_id }) => ({ data: assets.find(asset => asset.id === asset_id).data }),
      uploadReleaseAsset: async ({ name, data }) => {
        changes.push(['upload', name]);
        if (uploadFailure === name) { uploadFailure = undefined; throw new Error('Upload failed'); }
        assert.ok(!assets.some(asset => asset.name === name), `Duplicate upload ${name}`);
        return { data: addAsset(name, data) };
      },
      deleteReleaseAsset: async ({ asset_id }) => {
        const index = assets.findIndex(asset => asset.id === asset_id);
        changes.push(['delete', assets[index].name]);
        assets.splice(index, 1);
      },
      updateRelease: async params => { changes.push(['publish', params]); return { data: params }; },
      updateReleaseAsset: async ({ asset_id, label }) => {
        changes.push(['mark', asset_id]);
        assets.find(asset => asset.id === asset_id).label = label;
      },
    }, git: {
      getRef: async () => { if (!tagExists) throw { status: 404 }; return { data: { object: { type: 'commit', sha: tagSha } } }; },
      updateRef: async params => {
        if (tagFailure) { tagFailure = false; throw new Error('Tag update failed'); }
        tagSha = params.sha; changes.push(['tag', params]);
      },
      createRef: async params => changes.push(['createTag', params]),
    } },
  };
  return { ...f, assets, changes, releases, context, version, addAsset,
    stale: () => { f.git('commit', '--allow-empty', '-m', 'newer published build'); head = tagSha = f.git('rev-parse', 'HEAD'); },
    unbuiltHead: () => { f.git('commit', '--allow-empty', '-m', 'newer unbuilt commit'); head = f.git('rev-parse', 'HEAD'); },
    remoteHead: () => { head = remoteHead = 'new-remote-commit-not-in-checkout'; },
    remotePublished: () => { head = tagSha = remoteHead = 'new-remote-commit-not-in-checkout'; },
    published: value => { tagSha = value; },
    candidate: value => {
      context.sha = value;
      for (const os of Object.keys(NIGHTLY_PACKAGES)) {
        const path = join(f.root, `stage/${os}.json`);
        writeFileSync(path, JSON.stringify({ ...JSON.parse(readFileSync(path)), sha: value }));
      }
    },
    failTag: () => { tagFailure = true; },
    failUpload: name => { uploadFailure = name; }, missingTag: () => { tagExists = false; },
    run: () => publishNightly({ github, context, directory: join(f.root, 'stage'), cwd: f.root }),
  };
}

test('nightly publisher validates all platforms, publishes immutable updater URLs and fixed aliases, then tags commit', async (t) => {
  const f = publisherFixture(t);
  await f.run();
  const manifest = JSON.parse(f.assets.find(asset => asset.name === 'latest.json').data);
  assert.equal(manifest.version, f.version);
  assert.deepEqual(Object.keys(manifest.platforms), ['linux-x86_64', 'windows-x86_64', 'darwin-x86_64', 'darwin-aarch64']);
  assert.deepEqual(manifest.platforms['darwin-x86_64'], manifest.platforms['darwin-aarch64']);
  for (const entry of Object.values(manifest.platforms)) {
    const name = entry.url.split('/').at(-1);
    assert.ok(name.includes(f.version));
    assert.ok(f.assets.some(asset => asset.name === name));
    assert.match(entry.signature, /^signature-/);
  }
  for (const [os, type] of Object.entries(NIGHTLY_PACKAGES)) {
    assert.equal(f.assets.find(asset => asset.name === `Scorewing_${type.suffix}`).data.toString(), `new-${os}`);
  }
  const manifestUpload = f.changes.findIndex(change => change[0] === 'upload' && change[1] === 'latest.json');
  assert.ok(manifestUpload > f.changes.findIndex(change => change[0] === 'upload' && change[1].includes(f.version)));
  assert.equal(f.changes.find(change => change[0] === 'tag')[1].sha, f.context.sha);
  assert.equal(f.changes.find(change => change[0] === 'publish')[1].make_latest, 'false');
  await f.run();
  assert.equal(f.changes.filter(change => change[0] === 'upload' && change[1].includes(f.version)).length, 3,
    'Retries reuse matching immutable assets');
});

test('publisher skips superseded builds and refuses PRs, forks, other branches and stable releases before mutation', async (t) => {
  for (const change of [f => f.stale(), f => { f.context.eventName = 'pull_request'; },
    f => { f.context.repo.owner = 'fork'; }, f => { f.context.ref = 'refs/heads/feature'; },
    f => { f.releases[0].prerelease = false; }]) {
    const f = publisherFixture(t);
    change(f);
    try { await f.run(); } catch (error) { assert.ok(error instanceof assert.AssertionError); }
    assert.deepEqual(f.changes, []);
  }
});

test('publisher rejects incomplete or mixed snapshots and missing signatures before any public mutation', async (t) => {
  for (const change of [f => rmSync(join(f.root, 'stage/Windows.json')),
    f => { const path = join(f.root, 'stage/Linux.json'); const meta = JSON.parse(readFileSync(path)); meta.sha = 'other'; writeFileSync(path, JSON.stringify(meta)); },
    f => { const path = join(f.root, 'stage/macOS.json'); const meta = JSON.parse(readFileSync(path)); meta.signature = ''; writeFileSync(path, JSON.stringify(meta)); }]) {
    const f = publisherFixture(t);
    change(f);
    await assert.rejects(f.run());
    assert.deepEqual(f.changes, []);
  }
});

test('successful A publishes even when newer default-branch B is unbuilt or later fails', async (t) => {
  const f = publisherFixture(t);
  f.unbuiltHead();
  assert.deepEqual(await f.run(), { version: f.version, sha: f.context.sha });
  assert.equal(JSON.parse(f.assets.find(asset => asset.name === 'latest.json').data).version, f.version);
  assert.equal(f.changes.find(change => change[0] === 'tag')[1].sha, f.context.sha);
});

test('late successful A never overwrites an already published descendant B', async (t) => {
  const f = publisherFixture(t);
  f.stale();
  assert.deepEqual(await f.run(), { skipped: true });
  assert.deepEqual(f.changes, []);
});

test('ancestry checks tolerate remote commits published or pushed after the local checkout', async (t) => {
  const newerUnbuilt = publisherFixture(t);
  newerUnbuilt.remoteHead();
  assert.deepEqual(await newerUnbuilt.run(), { version: newerUnbuilt.version, sha: newerUnbuilt.context.sha });
  const newerPublished = publisherFixture(t);
  newerPublished.remotePublished();
  assert.deepEqual(await newerPublished.run(), { skipped: true });
  assert.deepEqual(newerPublished.changes, []);
});

test('publisher refuses divergence from published history and a commit removed from the default branch', async (t) => {
  const f = publisherFixture(t);
  f.git('checkout', '--orphan', 'diverged');
  f.git('commit', '--allow-empty', '-m', 'unrelated history');
  f.published(f.git('rev-parse', 'HEAD'));
  await assert.rejects(f.run(), /history diverged/);
  assert.deepEqual(f.changes, []);
  const removed = publisherFixture(t);
  removed.git('checkout', '--orphan', 'rewritten');
  removed.git('commit', '--allow-empty', '-m', 'rewritten history');
  removed.context.sha = removed.git('rev-parse', 'HEAD');
  await assert.rejects(removed.run(), /no longer in default-branch history/);
  assert.deepEqual(removed.changes, []);
});

test('publisher never replaces different immutable bytes and restores manifest after failed final upload', async (t) => {
  const collision = publisherFixture(t);
  collision.addAsset(`Scorewing_${collision.version}_linux_x64.AppImage`, 'different');
  await assert.rejects(collision.run(), /different bytes/);
  assert.deepEqual(collision.changes, []);
  const failed = publisherFixture(t);
  failed.failUpload('latest.json');
  await assert.rejects(failed.run(), /Upload failed/);
  assert.equal(JSON.parse(failed.assets.find(asset => asset.name === 'latest.json').data).version, 'old');
  assert.ok(failed.changes.every(change => !['tag', 'publish', 'mark'].includes(change[0])));
});

test('an advertised newer snapshot prevents regression even if moving its nightly tag failed', async (t) => {
  const f = publisherFixture(t);
  f.unbuiltHead();
  const middle = f.git('rev-parse', 'HEAD');
  f.unbuiltHead();
  const newest = f.git('rev-parse', 'HEAD');
  f.candidate(newest);
  f.failTag();
  await assert.rejects(f.run(), /Tag update failed/);
  assert.equal(JSON.parse(f.assets.find(asset => asset.name === 'latest.json').data).source_sha, newest);
  const previousChanges = f.changes.slice();
  f.candidate(middle);
  assert.deepEqual(await f.run(), { skipped: true });
  assert.deepEqual(f.changes, previousChanges, 'An older build cannot mutate an already advertised descendant');
});

test('publisher creates the first prerelease and nightly tag and retains twenty successful snapshots after publication', async (t) => {
  const first = publisherFixture(t);
  first.releases.length = 0;
  first.assets.length = 0;
  first.missingTag();
  await first.run();
  assert.equal(first.changes.find(change => change[0] === 'createRelease')[1].draft, true);
  assert.equal(first.changes.find(change => change[0] === 'createTag')[1].sha, first.context.sha);

  const retained = publisherFixture(t);
  for (let index = 1; index <= 22; index++) for (const [os, type] of Object.entries(NIGHTLY_PACKAGES)) {
    retained.addAsset(`Scorewing_0.2.3-nightly.${index}_${type.suffix}`, 'old-build', {
      created_at: `2026-01-${String(index).padStart(2, '0')}T00:00:00Z`,
      label: os === 'Linux' ? 'nightly-published' : '',
    });
  }
  await retained.run();
  assert.equal(retained.assets.filter(asset => /Scorewing_\d/.test(asset.name)).length, 60);
  assert.ok(!retained.assets.some(asset => asset.name.includes('0.2.3-nightly.1_')));
  assert.ok(retained.assets.some(asset => asset.name.includes(retained.version)));
  const published = retained.changes.findIndex(change => change[0] === 'publish');
  const firstPrune = retained.changes.findIndex(change => change[0] === 'delete' && change[1].includes('0.2.3-nightly.'));
  assert.ok(firstPrune > published, 'Never prune before publishing a complete new snapshot');
});

test('workflow grants nightly publishing writes only after the platform matrix succeeds', () => {
  const workflow = readFileSync(new URL('../.github/workflows/build.yml', import.meta.url), 'utf8');
  const platform = readFileSync(new URL('../.github/workflows/build-platforms.yml', import.meta.url), 'utf8');
  assert.match(workflow, /publish-nightly:\n    needs: build/);
  assert.match(workflow, /group: publish-nightly\n      cancel-in-progress: false/);
  assert.match(workflow, /group: publish-nightly\n      cancel-in-progress: false\n      queue: max/);
  assert.match(workflow, /group: build-.*github.run_id/);
  assert.match(platform, /Stamp nightly version and bundle configuration[\s\S]*?node tools\/nightly-version.mjs/);
  assert.match(platform, /inputs.nightly && '--config nightly.tauri.json'/);
  assert.match(platform, /TAURI_SIGNING_PRIVATE_KEY: \$\{\{ \(inputs.release \|\| inputs.nightly\)/);
});
