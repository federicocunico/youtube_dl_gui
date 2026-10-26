import { readFileSync, appendFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

export function validateVersions({ frontend, native, rust, lock, lockRoot }) {
  if (
    !/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z]+(?:[.-][0-9A-Za-z]+)*)?$/.test(
      frontend,
    )
  )
    throw new Error('Application version must be a release-compatible SemVer.');
  if ([native, rust, lock, lockRoot].some((version) => version !== frontend))
    throw new Error(
      'Package, npm lockfile, Tauri and Cargo versions must match.',
    );
  return frontend;
}

export function readVersion(root = process.cwd()) {
  const read = (name) => readFileSync(resolve(root, name), 'utf8');
  const frontend = JSON.parse(read('package.json')).version;
  const native = JSON.parse(read('src-tauri/tauri.conf.json')).version;
  const rust = /^version\s*=\s*"([^"]+)"/m.exec(
    read('src-tauri/Cargo.toml'),
  )?.[1];
  const lock = JSON.parse(read('package-lock.json'));
  return validateVersions({
    frontend,
    native,
    rust,
    lock: lock.version,
    lockRoot: lock.packages?.['']?.version,
  });
}

export function releasePlan({ version, event, ref, tagExists }) {
  const tag = `v${version}`;
  if (ref.startsWith('refs/tags/')) {
    if (ref !== `refs/tags/${tag}`)
      throw new Error('Tag and application version differ.');
    return { tag, publish: true };
  }
  return {
    tag,
    publish:
      ['push', 'workflow_dispatch'].includes(event) &&
      ref === 'refs/heads/main' &&
      !tagExists,
  };
}

function main() {
  const version = readVersion();
  const tag = `v${version}`;
  let tagExists = false;
  if (process.env.GITHUB_EVENT_NAME !== 'pull_request') {
    const remote = spawnSync(
      'git',
      ['ls-remote', '--exit-code', '--tags', 'origin', `refs/tags/${tag}`],
      { encoding: 'utf8', windowsHide: true },
    );
    if (remote.error || ![0, 2].includes(remote.status))
      throw new Error(`Cannot inspect remote tags: ${remote.stderr}`);
    tagExists = remote.status === 0;
  }
  const plan = releasePlan({
    version,
    event: process.env.GITHUB_EVENT_NAME,
    ref: process.env.GITHUB_REF || '',
    tagExists,
  });
  if (!process.env.GITHUB_OUTPUT)
    throw new Error('Release planning runs in GitHub Actions.');
  appendFileSync(
    process.env.GITHUB_OUTPUT,
    `tag=${plan.tag}\npublish=${plan.publish}\n`,
  );
  console.log(
    `${plan.tag}: ${plan.publish ? 'verified release required' : 'build verification only'}`,
  );
}
if (
  process.argv[1] &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url)
)
  main();
