import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync } from 'node:fs';
import { basename, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { readVersion } from './release-version.mjs';
import { validateReleaseProof, validateBuildRun } from './release-proof.mjs';

export function verifiedAssets(directory, version) {
  const required = [
    `YT-Saver_${version}_x64-setup.exe`,
    'runtime-lock.json',
    'engine-requirements.txt',
    'THIRD-PARTY-NOTICES.md',
    'inventory.json',
    'SIGNATURE.txt',
  ];
  const records = readFileSync(join(directory, 'SHA256SUMS.txt'), 'utf8')
    .trim()
    .split(/\r?\n/);
  const seen = new Set();
  for (const line of records) {
    const match = /^([0-9a-f]{64})  (.+)$/.exec(line);
    if (
      !match ||
      basename(match[2]) !== match[2] ||
      !required.includes(match[2]) ||
      seen.has(match[2])
    )
      throw new Error('Invalid distribution checksum manifest.');
    const digest = createHash('sha256')
      .update(readFileSync(join(directory, match[2])))
      .digest('hex');
    if (digest !== match[1])
      throw new Error(`Distribution checksum mismatch: ${match[2]}`);
    seen.add(match[2]);
  }
  if (seen.size !== required.length)
    throw new Error('Incomplete verified distribution.');
  return [...required, 'SHA256SUMS.txt'].map((name) => join(directory, name));
}

function command(program, args, allowed = [0]) {
  const result = spawnSync(program, args, {
    encoding: 'utf8',
    windowsHide: true,
  });
  if (result.error || !allowed.includes(result.status))
    throw new Error(
      `${program} failed: ${result.error?.message || result.stderr}`,
    );
  return result;
}

function main() {
  if (process.env.CI !== 'true')
    throw new Error('Publishing runs only in the verified CI release job.');
  const version = readVersion();
  const tag = `v${version}`;
  const assets = verifiedAssets(resolve('artifacts'), version);
  if (process.env.RELEASE_TAG !== tag)
    throw new Error('Unexpected release tag.');
  const head = command('git', ['rev-parse', 'HEAD']).stdout.trim();
  if (head !== process.env.VERIFIED_SHA)
    throw new Error(
      'Release checkout does not match the verified build commit.',
    );
  const event = JSON.parse(readFileSync(process.env.GITHUB_EVENT_PATH, 'utf8'));
  if (
    event.action !== 'yt-saver-release-validated' ||
    event.sender.login !== event.repository.owner.login ||
    event.repository.full_name !== process.env.GH_REPO
  )
    throw new Error(
      'Release validation must be submitted by the repository owner.',
    );
  const proof = event.client_payload;
  const installerSha = createHash('sha256')
    .update(readFileSync(assets[0]))
    .digest('hex');
  validateReleaseProof(proof, {
    version,
    commit: head,
    installerSha,
    engine: JSON.parse(readFileSync('artifacts/runtime-lock.json', 'utf8'))
      .engine,
    canaryUrls: JSON.parse(
      readFileSync('resources/canaries.json', 'utf8'),
    ).videos.map((item) => item.url),
  });
  const build = JSON.parse(
    command('gh', [
      'api',
      `repos/${process.env.GH_REPO}/actions/runs/${proof.run_id}`,
    ]).stdout,
  );
  validateBuildRun(build, { commit: head, repository: process.env.GH_REPO });
  const validationAsset = resolve('artifacts/LIVE-VALIDATION.json');
  writeFileSync(validationAsset, JSON.stringify(proof, null, 2) + '\n');
  assets.push(validationAsset);
  const proofDigest = createHash('sha256')
    .update(readFileSync(validationAsset))
    .digest('hex');
  const sums = readFileSync('artifacts/SHA256SUMS.txt', 'utf8').trimEnd();
  writeFileSync(
    'artifacts/SHA256SUMS.txt',
    `${sums}\n${proofDigest}  LIVE-VALIDATION.json\n`,
  );
  const existing = command(
    'git',
    ['rev-parse', '--verify', `refs/tags/${tag}^{commit}`],
    [0, 128],
  );
  if (existing.status === 0 && existing.stdout.trim() !== head) {
    throw new Error(
      'Existing tag points to another commit; refusing to move it.',
    );
  }
  if (existing.status !== 0) {
    command('git', ['config', 'user.name', 'github-actions[bot]']);
    command('git', [
      'config',
      'user.email',
      '41898282+github-actions[bot]@users.noreply.github.com',
    ]);
    command('git', ['tag', '-a', tag, head, '-m', `YT Saver ${tag}`]);
    command('git', ['push', 'origin', `refs/tags/${tag}`]);
  }
  const release = command(
    'gh',
    ['release', 'view', tag, '--json', 'isDraft,url'],
    [0, 1],
  );
  if (release.status === 0 && !JSON.parse(release.stdout).isDraft) {
    console.log(`Release already published: ${JSON.parse(release.stdout).url}`);
    return;
  }
  const notes = resolve('artifacts/RELEASE-NOTES.md');
  writeFileSync(
    notes,
    `YT Saver ${tag} for Windows x64.\n\nDownload and run the setup executable. Python, pip, FFmpeg and Deno are included.\n\nThis build passed deterministic tests, installed-app checks, real YouTube downloads and automatic rollback validation.\n\nUnsigned build. SHA-256 checksums, runtime inventory and third-party notices are attached.\n`,
  );
  if (release.status !== 0)
    command('gh', [
      'release',
      'create',
      tag,
      '--draft',
      '--verify-tag',
      '--title',
      `YT Saver ${tag}`,
      '--notes-file',
      notes,
      '--generate-notes',
      ...(version.includes('-') ? ['--prerelease'] : []),
    ]);
  command('gh', ['release', 'upload', tag, ...assets, '--clobber']);
  const uploaded = JSON.parse(
    command('gh', ['release', 'view', tag, '--json', 'assets']).stdout,
  ).assets;
  const expected = new Set(assets.map((path) => basename(path)));
  if (
    uploaded.length !== expected.size ||
    uploaded.some((asset) => !expected.has(asset.name))
  )
    throw new Error(
      'Draft release assets do not match the verified distribution.',
    );
  command('gh', [
    'release',
    'edit',
    tag,
    '--draft=false',
    ...(version.includes('-') ? [] : ['--latest']),
  ]);
  console.log(command('gh', ['release', 'view', tag, '--json', 'url']).stdout);
}
if (
  process.argv[1] &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url)
)
  main();
