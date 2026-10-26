export function assertLiveReports(canaries, rollback) {
  if (
    !Array.isArray(canaries) ||
    canaries.length !== 2 ||
    new Set(canaries.map((item) => item.url)).size !== 2 ||
    canaries.some((item) => item.status !== 'pass') ||
    rollback?.status !== 'pass' ||
    rollback.automatic_rollback !== true ||
    rollback.baseline_active_during_promotion !== true ||
    rollback.recovery_retries !== 1
  )
    throw new Error(
      'Live validation incomplete, failed or inconclusive. Release publication is blocked.',
    );
}
