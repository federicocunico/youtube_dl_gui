import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readdirSync } from 'node:fs';
import { resolve, join } from 'node:path';
const installer = resolve(process.argv[2] || '');
if (!existsSync(installer) || !installer.endsWith('-setup.exe'))
  throw new Error('Pass the compiled NSIS setup executable.');
const root = resolve('.runtime-cache/Frame smoke è');
mkdirSync(root, { recursive: true });
function run(command, args, env = process.env) {
  const result = spawnSync(command, args, {
    stdio: 'inherit',
    windowsHide: true,
    env,
    timeout: 600000,
  });
  if (result.error || result.status !== 0)
    throw new Error(
      `${command} failed: ${result.error?.message || result.status}`,
    );
}
const installation = spawnSync(installer, ['/S', `/D=${root}`], {
  windowsVerbatimArguments: true,
  windowsHide: true,
  timeout: 180000,
});
if (installation.error || installation.status !== 0)
  throw new Error('Silent install failed.');
try {
  run(join(root, 'runtime/python/python.exe'), [
    '-I',
    'scripts/package-smoke.py',
    '--root',
    root,
  ]);
  run(
    'cargo',
    ['test', '--manifest-path', 'src-tauri/Cargo.toml', '--locked'],
    { ...process.env, FRAME_TEST_RESOURCES: root },
  );
  if (process.env.FRAME_VERIFY_LIVE === 'true')
    run(process.execPath, ['scripts/verify-canaries.mjs'], {
      ...process.env,
      FRAME_TEST_RESOURCES: root,
    });
  if (process.env.CI === 'true')
    run(process.execPath, ['scripts/desktop-smoke.mjs', root]);
  else
    console.log(
      'Native UI startup not performed locally; CI runs the isolated WebView2 smoke.',
    );
} finally {
  const uninstall = readdirSync(root).find((name) =>
    /^uninstall.*\.exe$/i.test(name),
  );
  if (!uninstall) throw new Error('Installed NSIS uninstaller is missing.');
  const removal = spawnSync(join(root, uninstall), ['/S'], {
    windowsHide: true,
    timeout: 120000,
  });
  if (removal.error || removal.status !== 0)
    throw new Error('Silent uninstall failed.');
  const deadline = Date.now() + 20000;
  while (existsSync(join(root, 'Frame.exe')) && Date.now() < deadline)
    await new Promise((resolve) => setTimeout(resolve, 250));
  if (existsSync(join(root, 'Frame.exe')))
    throw new Error('Uninstall did not remove the installed executable.');
  console.log('Silent uninstall verified.');
}
