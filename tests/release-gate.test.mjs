import { test } from 'node:test';
import assert from 'node:assert/strict';
import { assertLiveReports } from '../scripts/release-gate.mjs';

const canaries = [
  { url: 'first', status: 'pass' },
  { url: 'second', status: 'pass' },
];
const rollback = {
  status: 'pass',
  automatic_rollback: true,
  baseline_active_during_promotion: true,
  recovery_retries: 1,
};
test('release accepts both successful sources and the complete rollback proof', () => {
  assert.doesNotThrow(() => assertLiveReports(canaries, rollback));
});
test('failed, inconclusive, duplicate and absent evidence blocks release', () => {
  for (const status of ['fail', 'inconclusive', undefined]) {
    assert.throws(() =>
      assertLiveReports([canaries[0], { ...canaries[1], status }], rollback),
    );
    assert.throws(() => assertLiveReports(canaries, { ...rollback, status }));
  }
  assert.throws(() => assertLiveReports([], rollback));
  assert.throws(() => assertLiveReports([canaries[0], canaries[0]], rollback));
  assert.throws(() =>
    assertLiveReports(canaries, { ...rollback, recovery_retries: 2 }),
  );
  assert.throws(() => assertLiveReports(canaries, undefined));
});
