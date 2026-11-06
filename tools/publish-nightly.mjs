import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { NIGHTLY_PACKAGES } from './stage-nightly.mjs';
import { releaseNotes } from './release-notes.mjs';

const digest = data => `sha256:${createHash('sha256').update(data).digest('hex')}`;

export async function publishNightly({ github, context, directory, cwd = process.cwd() }) {
  assert.equal(context.eventName, 'push', 'Nightlies only publish branch pushes');
  assert.equal(`${context.repo.owner}/${context.repo.repo}`, 'Popchor-RUB/Scorewing');
  const branch = context.payload.repository.default_branch;
  assert.equal(context.ref, `refs/heads/${branch}`, 'Nightlies only publish the default branch');
  const repo = context.repo;
  const ancestor = async (older, newer) => {
    const { data } = await github.rest.repos.compareCommits({ ...repo, base: older, head: newer });
    return data.status === 'ahead' || data.status === 'identical';
  };
  const head = (await github.rest.repos.getBranch({ ...repo, branch })).data.commit.sha;
  assert.ok(await ancestor(context.sha, head), 'The built commit is no longer in default-branch history');
  const releases = await github.paginate(github.rest.repos.listReleases, { ...repo, per_page: 100 });
  let release = releases.find(item => item.tag_name === 'nightly');
  if (release) assert.ok(release.prerelease, 'Refuse to mutate a stable release tagged nightly');
  const assets = release ? await github.paginate(github.rest.repos.listReleaseAssets,
    { ...repo, release_id: release.id, per_page: 100 }) : [];
  const download = async asset => Buffer.from((await github.rest.repos.getReleaseAsset({ ...repo, asset_id: asset.id,
    headers: { accept: 'application/octet-stream' } })).data);
  const advertised = assets.find(asset => asset.name === 'latest.json');
  let source;
  if (release && !release.draft && advertised) {
    source = JSON.parse((await download(advertised)).toString('utf8')).source_sha;
    if (source !== undefined) assert.match(source, /^[a-f0-9]{40}$/, 'Published manifest must identify its source commit');
  }
  let tag;
  try {
    tag = (await github.rest.git.getRef({ ...repo, ref: 'tags/nightly' })).data.object;
  } catch (error) {
    if (error.status !== 404) throw error;
  }
  while (tag?.type === 'tag') tag = (await github.rest.git.getTag({ ...repo, tag_sha: tag.sha })).data.object;
  // The manifest is authoritative even if an earlier tag update failed after publication.
  const published = source ?? tag?.sha;
  if (release && !release.draft && published && published !== context.sha) {
    if (await ancestor(context.sha, published)) return { skipped: true };
    assert.ok(await ancestor(published, context.sha), 'Published nightly history diverged; resolve the branch history before publishing');
  }

  // Validate every package and signature before changing the public release.
  const packages = Object.entries(NIGHTLY_PACKAGES).map(([os, type]) => {
    const metadata = JSON.parse(readFileSync(join(directory, `${os}.json`), 'utf8'));
    assert.equal(metadata.os, os);
    assert.equal(metadata.sha, context.sha, 'All packages must come from this commit');
    assert.match(metadata.version, /^\d+\.\d+\.\d+-nightly\.[1-9]\d*$/);
    assert.equal(metadata.name, `Scorewing_${metadata.version}_${type.suffix}`);
    assert.ok(typeof metadata.signature === 'string' && metadata.signature.trim());
    const data = readFileSync(join(directory, metadata.name));
    assert.ok(data.length, 'Nightly packages must not be empty');
    return { ...metadata, ...type, data };
  });
  const version = packages[0].version;
  assert.ok(packages.every(packageType => packageType.version === version), 'All platforms must have the same version');
  const notes = await releaseNotes({ github, repo, nightly: true, sha: context.sha, cwd });
  if (!release) release = (await github.rest.repos.createRelease({ ...repo, tag_name: 'nightly', target_commitish: context.sha,
    name: 'Nightly builds', body: notes, draft: true, prerelease: true, make_latest: 'false' })).data;
  const params = { ...repo, release_id: release.id };
  const upload = async (name, data, contentType = 'application/octet-stream') => (await github.rest.repos.uploadReleaseAsset({
    ...repo, url: release.upload_url, name, data,
    headers: { 'content-type': contentType, 'content-length': data.length },
  })).data;
  const remove = asset => github.rest.repos.deleteReleaseAsset({ ...repo, asset_id: asset.id });

  // Versioned assets are immutable: cached manifests continue to verify after a replacement.
  for (const packageType of packages) {
    const existing = assets.find(asset => asset.name === packageType.name);
    if (existing) assert.equal(existing.digest || digest(await download(existing)), digest(packageType.data),
      'Refuse to replace a versioned nightly package with different bytes');
    else assets.push(await upload(packageType.name, packageType.data));
  }
  const replace = async (name, data, contentType) => {
    const existing = assets.find(asset => asset.name === name);
    const previous = existing && await download(existing);
    if (existing) await remove(existing);
    try {
      const uploaded = await upload(name, data, contentType);
      if (existing) assets.splice(assets.indexOf(existing), 1);
      assets.push(uploaded);
    } catch (error) {
      if (previous) await upload(name, previous, contentType);
      throw error;
    }
  };
  for (const packageType of packages) await replace(`Scorewing_${packageType.suffix}`, packageType.data);
  const base = `https://github.com/${repo.owner}/${repo.repo}/releases/download/nightly/`;
  const manifest = { version, source_sha: context.sha, notes, pub_date: new Date().toISOString(), platforms: {} };
  for (const packageType of packages) for (const platform of packageType.platforms) {
    manifest.platforms[platform] = { signature: packageType.signature, url: base + packageType.name };
  }
  await replace('latest.json', Buffer.from(`${JSON.stringify(manifest, null, 2)}\n`), 'application/json');
  if (tag) await github.rest.git.updateRef({ ...repo, ref: 'tags/nightly', sha: context.sha, force: true });
  else {
    await github.rest.git.createRef({ ...repo, ref: 'refs/tags/nightly', sha: context.sha });
  }
  await github.rest.repos.updateRelease({ ...params, name: 'Nightly builds', body: notes,
    target_commitish: context.sha, draft: false, prerelease: true, make_latest: 'false' });
  const marker = assets.find(asset => asset.name === packages[0].name);
  await github.rest.repos.updateReleaseAsset({ ...repo, asset_id: marker.id, label: 'nightly-published' });
  marker.label = 'nightly-published';

  // Prune only complete snapshots, after the new manifest and release are public.
  const versions = new Map();
  for (const asset of assets) {
    const match = asset.name.match(/^Scorewing_(\d+\.\d+\.\d+-nightly\.[1-9]\d*)_(.+)$/);
    if (match && Object.values(NIGHTLY_PACKAGES).some(type => type.suffix === match[2])) {
      const group = versions.get(match[1]) || [];
      group.push(asset);
      versions.set(match[1], group);
    }
  }
  const complete = [...versions].filter(([, group]) => group.length === 3 && group.some(asset => asset.label === 'nightly-published'))
    .sort(([a, aa], [b, bb]) => a === version ? -1 : b === version ? 1
      : Math.max(...bb.map(asset => Date.parse(asset.created_at))) - Math.max(...aa.map(asset => Date.parse(asset.created_at))));
  for (const [, group] of complete.slice(20)) for (const asset of group) await remove(asset);
  return { version, sha: context.sha };
}
