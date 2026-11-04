import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

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
