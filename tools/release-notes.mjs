import { execFileSync } from 'node:child_process';

export async function releaseNotes({ github, repo, tag, nightly = false, sha = 'HEAD', cwd = process.cwd() }) {
  if (!nightly) {
    const releases = await github.paginate(github.rest.repos.listReleases, { ...repo, per_page: 100 });
    const existing = releases.find(release => release.tag_name === tag);
    if (existing) {
      if (!existing.draft) throw new Error('This release is already published; use a new version tag.');
      return existing.body || '';
    }
  }
  let range = sha;
  try {
    const { data: previous } = await github.rest.repos.getLatestRelease(repo);
    range = `refs/tags/${previous.tag_name}..${sha}`;
  } catch (error) {
    if (error.status !== 404) throw error;
  }
  const commits = execFileSync('git', ['log', '--reverse', '--no-merges', '--format=- %s', range, '--'],
    { cwd, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trimEnd();
  const intro = nightly ? 'Latest successful default-branch build. This prerelease is replaced automatically.'
    : 'Review all platform installers and latest.json before publishing this release.';
  return `${intro}\n\n## Commits\n\n${commits || 'No new non-merge commits.'}\n`;
}
