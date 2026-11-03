import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const workflow = readFileSync(new URL('../.github/workflows/build-platforms.yml', import.meta.url), 'utf8');
const source = workflow.match(/- name: Remove standalone updater signatures[\s\S]*?script: \|\n((?: {12}.*\n)+)/)[1];
const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;
const cleanup = new AsyncFunction('require', 'github', 'context', 'process', source);

function fixture() {
  const names = ['Scorewing_linux.AppImage', 'Scorewing_windows-setup.exe',
    'Scorewing_darwin_x64.app.tar.gz', 'Scorewing_darwin_aarch64.app.tar.gz'];
  const assets = ['latest.json', ...names, ...names.map(name => `${name}.sig`)].map((name, id) => ({
    id, name, browser_download_url: `https://github.com/Popchor-RUB/Scorewing/releases/download/v1.0.0/${name}`,
  }));
  const manifest = { platforms: Object.fromEntries(names.map((name, i) => [name, {
    url: assets[i + 1].browser_download_url, signature: `signature-${i}`,
  }])) };
  const release = { draft: true };
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

test('removes only standalone signatures after validating embedded signatures and download URLs', async () => {
  const f = fixture();
  await f.run();
  assert.deepEqual(f.deleted, f.assets.filter(asset => asset.name.endsWith('.sig')).map(asset => asset.id));
});

test('preserves all assets when the manifest or an updater signature or download is missing', async () => {
  const invalid = [
    f => { f.assets.shift(); },
    f => { f.manifest.platforms = {}; },
    f => { Object.values(f.manifest.platforms)[0].signature = ''; },
    f => { Object.values(f.manifest.platforms)[0].url = 'https://example.com/missing.AppImage'; },
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
