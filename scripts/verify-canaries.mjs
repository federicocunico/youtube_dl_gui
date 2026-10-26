import { spawnSync } from 'node:child_process';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { join } from 'node:path';

const root = fileURLToPath(new URL('../', import.meta.url));
mkdirSync(join(root, 'test-results'), { recursive: true });
const result = spawnSync(
  'cargo',
  [
    'test',
    '--manifest-path',
    'src-tauri/Cargo.toml',
    '--locked',
    'updater::tests::live_',
    '--',
    '--ignored',
    '--nocapture',
  ],
  {
    cwd: root,
    stdio: 'inherit',
    windowsHide: true,
    env: {
      ...process.env,
      FRAME_LIVE_REPORT: join(root, 'test-results', 'canary-report.json'),
      FRAME_UPDATE_REPORT: join(
        root,
        'test-results',
        'update-rollback-report.json',
      ),
    },
    timeout: 360000,
  },
);
if (result.error || result.status !== 0)
  throw new Error(
    'Live YouTube validation failed or was inconclusive. Release publication is blocked.',
  );
const report = JSON.parse(
  readFileSync(join(root, 'test-results', 'canary-report.json'), 'utf8'),
);
const rollback = JSON.parse(
  readFileSync(
    join(root, 'test-results', 'update-rollback-report.json'),
    'utf8',
  ),
);
if (
  !report.every((item) => item.status === 'pass') ||
  rollback.status !== 'pass'
)
  throw new Error('Incomplete canary report');
console.log('All live YouTube canaries passed.');
