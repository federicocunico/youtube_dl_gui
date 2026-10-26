import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync, rmSync, realpathSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname, basename } from 'node:path';
import { createHash } from 'node:crypto';
import {
  releasePlan,
  validateVersions,
  readVersion,
} from '../scripts/release-version.mjs';
import { verifiedAssets } from '../scripts/publish-release.mjs';

test('all shipped version manifests agree and invalid release versions fail', () => {
  assert.match(readVersion(), /^\d+\.\d+\.\d+/);
  const versions = {
    frontend: '1.2.3',
    native: '1.2.3',
    rust: '1.2.3',
    lock: '1.2.3',
    lockRoot: '1.2.3',
  };
  assert.equal(validateVersions(versions), '1.2.3');
  for (const key of Object.keys(versions))
    assert.throws(() => validateVersions({ ...versions, [key]: '1.2.4' }));
  for (const frontend of ['1.2', '01.2.3', 'v1.2.3', '1.2.3/other'])
    assert.throws(() => validateVersions({ ...versions, frontend }));
});
test('new main versions automatically release; PRs, other branches and existing versions only build', () => {
  const input = {
    version: '1.2.3',
    event: 'push',
    ref: 'refs/heads/main',
    tagExists: false,
  };
  assert.deepEqual(releasePlan(input), { tag: 'v1.2.3', publish: true });
  assert.equal(releasePlan({ ...input, tagExists: true }).publish, false);
  assert.equal(releasePlan({ ...input, event: 'pull_request' }).publish, false);
  assert.equal(
    releasePlan({ ...input, ref: 'refs/heads/feature' }).publish,
    false,
  );
  assert.equal(
    releasePlan({ ...input, event: 'workflow_dispatch' }).publish,
    true,
  );
  assert.equal(
    releasePlan({ ...input, ref: 'refs/tags/v1.2.3', tagExists: true }).publish,
    true,
  );
  assert.throws(() => releasePlan({ ...input, ref: 'refs/tags/v9.0.0' }));
});
test('release refuses corrupted, incomplete and path-traversing artifact manifests', () => {
  const directory = mkdtempSync(join(tmpdir(), 'yt-saver-release-'));
  const names = [
    'YT-Saver_1.2.3_x64-setup.exe',
    'runtime-lock.json',
    'engine-requirements.txt',
    'THIRD-PARTY-NOTICES.md',
    'inventory.json',
    'SIGNATURE.txt',
  ];
  const digest = createHash('sha256').update('verified').digest('hex');
  const lines = names.map((name) => `${digest}  ${name}`);
  try {
    for (const name of names) writeFileSync(join(directory, name), 'verified');
    const manifest = join(directory, 'SHA256SUMS.txt');
    writeFileSync(manifest, lines.join('\n'));
    assert.equal(verifiedAssets(directory, '1.2.3').length, 7);
    writeFileSync(manifest, lines.slice(1).join('\n'));
    assert.throws(() => verifiedAssets(directory, '1.2.3'));
    writeFileSync(manifest, `${digest}  ../outside.exe`);
    assert.throws(() => verifiedAssets(directory, '1.2.3'));
    writeFileSync(manifest, lines.join('\n'));
    writeFileSync(join(directory, names[0]), 'tampered');
    assert.throws(() => verifiedAssets(directory, '1.2.3'));
  } finally {
    const resolved = realpathSync(directory);
    if (
      dirname(resolved) !== realpathSync(tmpdir()) ||
      !basename(resolved).startsWith('yt-saver-release-')
    )
      throw new Error('Refusing cleanup outside the generated test directory.');
    rmSync(resolved, { recursive: true });
  }
});
