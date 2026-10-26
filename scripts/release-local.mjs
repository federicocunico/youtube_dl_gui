// Single-command release coordinator. The exact hosted-CI installer is tested
// locally because YouTube rejects requests from GitHub's data-center IPs.
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { readVersion } from './release-version.mjs';
import { verifiedAssets } from './publish-release.mjs';
import { validateReleaseProof, validateBuildRun } from './release-proof.mjs';

function run(program, args, inherit = false, env = process.env, allowed = [0]) {
  const result = spawnSync(program, args, {
    encoding: 'utf8',
    windowsHide: true,
    stdio: inherit ? 'inherit' : 'pipe',
    env,
    maxBuffer: 10 * 1024 * 1024,
  });
  if (result.error || !allowed.includes(result.status))
    throw new Error(
      `${program} failed: ${result.error?.message || result.stderr || result.status}`,
    );
  return result;
}
const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
async function waitRun(id) {
  const deadline = Date.now() + 3600000;
  while (Date.now() < deadline) {
    const state = JSON.parse(
      run('gh', ['run', 'view', String(id), '--json', 'status,conclusion'])
        .stdout,
    );
    if (state.status === 'completed') {
      if (state.conclusion !== 'success') {
        run('gh', ['run', 'view', String(id), '--log-failed'], true);
        throw new Error(`Workflow ${id} ended with ${state.conclusion}.`);
      }
      return;
    }
    await delay(10000);
  }
  throw new Error(`Timed out monitoring workflow ${id}.`);
}
const version = readVersion();
if (run('git', ['branch', '--show-current']).stdout.trim() !== 'main')
  throw new Error('Release from main.');
run('git', ['diff', '--quiet']);
run('git', ['diff', '--cached', '--quiet']);
const commit = run('git', ['rev-parse', 'HEAD']).stdout.trim();
const repository = JSON.parse(
  run('gh', ['repo', 'view', '--json', 'nameWithOwner']).stdout,
).nameWithOwner;
const login = JSON.parse(run('gh', ['api', 'user']).stdout).login;
if (login !== repository.split('/')[0])
  throw new Error(
    'Submit release validation using the repository owner account.',
  );
const existing = run(
  'gh',
  ['release', 'view', `v${version}`, '--json', 'isDraft,url'],
  false,
  process.env,
  [0, 1],
);
if (existing.status === 0 && !JSON.parse(existing.stdout).isDraft) {
  console.log(`Already published: ${JSON.parse(existing.stdout).url}`);
  process.exit(0);
}
run('git', ['push', 'origin', 'main'], true);
let build;
for (let attempt = 0; attempt < 30; attempt++) {
  const runs = JSON.parse(
    run('gh', [
      'run',
      'list',
      '--workflow',
      'windows.yml',
      '--event',
      'push',
      '--branch',
      'main',
      '--commit',
      commit,
      '--limit',
      '5',
      '--json',
      'databaseId,createdAt',
    ]).stdout,
  );
  build = runs[0];
  if (build) break;
  await delay(5000);
}
if (!build) throw new Error('No Windows build found for the release commit.');
console.log(`Waiting for verified Windows build ${build.databaseId}.`);
await waitRun(build.databaseId);
const metadata = JSON.parse(
  run('gh', ['api', `repos/${repository}/actions/runs/${build.databaseId}`])
    .stdout,
);
validateBuildRun(metadata, { commit, repository });
mkdirSync(resolve('.runtime-cache'), { recursive: true });
const directory = mkdtempSync(
  resolve('.runtime-cache', `release-${build.databaseId}-`),
);
console.log('Downloading the exact CI distribution.');
run(
  'gh',
  [
    'run',
    'download',
    String(build.databaseId),
    '--name',
    'YT-Saver-windows-x64',
    '--dir',
    directory,
  ],
  true,
);
const installer = verifiedAssets(directory, version)[0];
const installerSha = createHash('sha256')
  .update(readFileSync(installer))
  .digest('hex');
console.log(
  'Testing the installed CI package, real YouTube downloads and automatic rollback locally.',
);
run(process.execPath, ['scripts/smoke-install.mjs', installer], true, {
  ...process.env,
  FRAME_VERIFY_LIVE: 'true',
});
run('git', ['diff', '--quiet']);
run('git', ['diff', '--cached', '--quiet']);
if (run('git', ['rev-parse', 'HEAD']).stdout.trim() !== commit)
  throw new Error('Release source changed during validation.');
const proof = {
  schema: 1,
  commit,
  run_id: String(build.databaseId),
  version,
  installer_sha256: installerSha,
  validated_at: new Date().toISOString(),
  canaries: JSON.parse(readFileSync('test-results/canary-report.json', 'utf8')),
  rollback: JSON.parse(
    readFileSync('test-results/update-rollback-report.json', 'utf8'),
  ),
};
validateReleaseProof(proof, {
  version,
  commit,
  installerSha,
  engine: JSON.parse(readFileSync(join(directory, 'runtime-lock.json'), 'utf8'))
    .engine,
  canaryUrls: JSON.parse(
    readFileSync('resources/canaries.json', 'utf8'),
  ).videos.map((item) => item.url),
});
const request = join(directory, 'release-proof.json');
writeFileSync(
  request,
  JSON.stringify(
    { event_type: 'yt-saver-release-validated', client_payload: proof },
    null,
    2,
  ),
);
const dispatched = Date.now();
run(
  'gh',
  [
    'api',
    '--method',
    'POST',
    `repos/${repository}/dispatches`,
    '--input',
    request,
  ],
  true,
);
let publication;
for (let attempt = 0; attempt < 30; attempt++) {
  const runs = JSON.parse(
    run('gh', [
      'run',
      'list',
      '--workflow',
      'windows.yml',
      '--event',
      'repository_dispatch',
      '--limit',
      '5',
      '--json',
      'databaseId,createdAt',
    ]).stdout,
  );
  publication = runs.find(
    (item) => Date.parse(item.createdAt) >= dispatched - 10000,
  );
  if (publication) break;
  await delay(5000);
}
if (!publication)
  throw new Error('Release publication workflow did not start.');
console.log(
  `Monitoring automatic tag and release publication ${publication.databaseId}.`,
);
await waitRun(publication.databaseId);
const published = JSON.parse(
  run('gh', ['release', 'view', `v${version}`, '--json', 'url,tagName,isDraft'])
    .stdout,
);
if (published.isDraft || published.tagName !== `v${version}`)
  throw new Error('Release was not published.');
run(
  'git',
  ['fetch', 'origin', `refs/tags/v${version}:refs/tags/v${version}`],
  true,
);
if (
  run('git', ['rev-parse', `refs/tags/v${version}^{commit}`]).stdout.trim() !==
  commit
)
  throw new Error('Published tag does not match the validated source.');
console.log(published.url);
