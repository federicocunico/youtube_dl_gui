import { spawnSync } from 'node:child_process';
import { dirname, join, resolve } from 'node:path';
const npm =
  process.env.npm_execpath ||
  join(dirname(process.execPath), 'node_modules/npm/bin/npm-cli.js');
function run(command, args) {
  const result = spawnSync(command, args, {
    stdio: 'inherit',
    windowsHide: true,
  });
  if (result.error || result.status !== 0) process.exit(result.status || 1);
}
run(process.execPath, [npm, 'run', 'check']);
run(process.execPath, [npm, 'test']);
run(process.execPath, [
  '--test',
  'tests/release-gate.test.mjs',
  'tests/release-automation.test.mjs',
]);
run(resolve('resources/runtime/python/python.exe'), [
  '-I',
  '-m',
  'unittest',
  'discover',
  '-s',
  'tests',
  '-p',
  'test_*.py',
  '-v',
]);
run('cargo', [
  'fmt',
  '--manifest-path',
  'src-tauri/Cargo.toml',
  '--',
  '--check',
]);
run('cargo', ['test', '--manifest-path', 'src-tauri/Cargo.toml', '--locked']);
run(process.execPath, [npm, 'run', 'test:ui']);
