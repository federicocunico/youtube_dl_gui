import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  validateReleaseProof,
  validateBuildRun,
} from '../scripts/release-proof.mjs';

const context = {
  version: '1.2.3',
  commit: 'a'.repeat(40),
  installerSha: 'b'.repeat(64),
  engine: '2026.10.21',
  canaryUrls: ['first', 'second'],
  now: Date.now(),
};
const proof = {
  schema: 1,
  version: context.version,
  commit: context.commit,
  run_id: '1234',
  installer_sha256: context.installerSha,
  validated_at: new Date(context.now).toISOString(),
  canaries: [
    { url: 'first', status: 'pass' },
    { url: 'second', status: 'pass' },
  ],
  rollback: {
    status: 'pass',
    version: context.engine,
    automatic_rollback: true,
    baseline_active_during_promotion: true,
    recovery_retries: 1,
  },
};
test('fresh real-download proof is bound to source, installer, engine and registered sources', () => {
  assert.doesNotThrow(() => validateReleaseProof(proof, context));
  for (const invalid of [
    { ...proof, commit: 'c'.repeat(40) },
    { ...proof, installer_sha256: 'd'.repeat(64) },
    { ...proof, validated_at: new Date(context.now - 86400001).toISOString() },
    { ...proof, validated_at: new Date(context.now + 600000).toISOString() },
    {
      ...proof,
      canaries: [
        { url: 'first', status: 'pass' },
        { url: 'second', status: 'inconclusive' },
      ],
    },
    {
      ...proof,
      canaries: [
        { url: 'first', status: 'pass' },
        { url: 'unregistered', status: 'pass' },
      ],
    },
    { ...proof, rollback: { ...proof.rollback, version: 'other' } },
    { ...proof, run_id: '../other' },
  ])
    assert.throws(() => validateReleaseProof(invalid, context));
});
test('publication accepts only a successful main push build for the exact repository and commit', () => {
  const run = {
    head_sha: context.commit,
    repository: { full_name: 'owner/repo' },
    path: '.github/workflows/windows.yml',
    event: 'push',
    head_branch: 'main',
    status: 'completed',
    conclusion: 'success',
  };
  const expected = { commit: context.commit, repository: 'owner/repo' };
  assert.doesNotThrow(() => validateBuildRun(run, expected));
  for (const invalid of [
    { ...run, head_sha: 'c'.repeat(40) },
    { ...run, event: 'pull_request' },
    { ...run, conclusion: 'failure' },
    { ...run, status: 'in_progress' },
    { ...run, path: '.github/workflows/other.yml' },
    { ...run, repository: { full_name: 'other/repo' } },
  ])
    assert.throws(() => validateBuildRun(invalid, expected));
});
