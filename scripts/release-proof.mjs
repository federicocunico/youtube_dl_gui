import { assertLiveReports } from './release-gate.mjs';

export function validateReleaseProof(
  proof,
  { version, commit, installerSha, engine, canaryUrls, now = Date.now() },
) {
  const timestamp = Date.parse(proof?.validated_at);
  if (
    proof?.schema !== 1 ||
    proof.version !== version ||
    !/^[0-9a-f]{40}$/.test(proof.commit || '') ||
    proof.commit !== commit ||
    !/^\d+$/.test(String(proof.run_id || '')) ||
    !/^[0-9a-f]{64}$/.test(proof.installer_sha256 || '') ||
    proof.installer_sha256 !== installerSha ||
    !Number.isFinite(timestamp) ||
    timestamp > now + 300000 ||
    now - timestamp > 86400000
  )
    throw new Error(
      'Release proof is stale or does not match the verified installer and commit.',
    );
  assertLiveReports(proof.canaries, proof.rollback);
  if (
    proof.rollback.version !== engine ||
    proof.canaries.some((item) => !canaryUrls.includes(item.url))
  )
    throw new Error(
      'Release proof uses another engine or unregistered media sources.',
    );
}

export function validateBuildRun(run, { commit, repository }) {
  if (
    run.head_sha !== commit ||
    run.repository?.full_name !== repository ||
    run.path !== '.github/workflows/windows.yml' ||
    run.event !== 'push' ||
    run.head_branch !== 'main' ||
    run.status !== 'completed' ||
    run.conclusion !== 'success'
  )
    throw new Error(
      'Release requires a successful main-branch Windows build of the exact commit.',
    );
}
