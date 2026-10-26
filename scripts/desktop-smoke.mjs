// Native WebView2 rendering on an isolated CI runner. Never opens UI on the user's desktop.
import { chromium } from '@playwright/test';
import { spawn, spawnSync } from 'node:child_process';
import { mkdirSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { createServer } from 'node:net';
if (process.env.CI !== 'true')
  throw new Error('Native UI smoke runs only on the isolated CI runner.');
const root = resolve(process.argv[2] || '');
const reports = resolve('test-results/native');
mkdirSync(reports, { recursive: true });
const listener = createServer();
await new Promise((r) => listener.listen(0, '127.0.0.1', r));
const port = listener.address().port;
await new Promise((r) => listener.close(r));
// Elevated WebView2 hosts ignore environment/HKCU browser flags. GitHub's
// Windows runner is elevated; scope the documented HKLM override to Frame.exe.
// https://learn.microsoft.com/microsoft-edge/webview2/concepts/security
const registry = join(process.env.SystemRoot, 'System32/reg.exe');
const policy =
  'HKLM\\Software\\Policies\\Microsoft\\Edge\\WebView2\\AdditionalBrowserArguments';
const configured = spawnSync(
  registry,
  [
    'add',
    policy,
    '/v',
    'Frame.exe',
    '/t',
    'REG_SZ',
    '/d',
    `--remote-debugging-port=${port} --disable-gpu`,
    '/f',
  ],
  { encoding: 'utf8', windowsHide: true },
);
if (configured.error || configured.status !== 0)
  throw new Error(
    `Cannot configure isolated CI WebView2 debugging: ${configured.stderr}`,
  );
const child = spawn(join(root, 'Frame.exe'), [], {
  windowsHide: true,
  env: {
    ...process.env,
    PATH: '',
    WEBVIEW2_USER_DATA_FOLDER: resolve('.runtime-cache/native-webview-profile'),
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --disable-gpu`,
  },
  stdio: ['ignore', 'pipe', 'pipe'],
});
let output = '';
for (const stream of [child.stdout, child.stderr])
  stream.on('data', (data) => {
    output = (output + data).slice(-65536);
  });
let lastConnectionError;
let browser;
try {
  for (let attempt = 0; attempt < 240; attempt++) {
    if (child.exitCode !== null)
      throw new Error(`Native application exited: ${child.exitCode}`);
    try {
      browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, {
        timeout: 1000,
      });
      break;
    } catch (error) {
      lastConnectionError = String(error);
      await new Promise((r) => setTimeout(r, 250));
    }
  }
  if (!browser) {
    writeFileSync(
      join(reports, 'startup.log'),
      output + '\n' + lastConnectionError,
    );
    const diagnostic = spawnSync(
      join(
        process.env.SystemRoot,
        'System32/WindowsPowerShell/v1.0/powershell.exe',
      ),
      [
        '-NoProfile',
        '-NonInteractive',
        '-Command',
        `Get-CimInstance Win32_Process | Where-Object { $_.Name -match '^(Frame|msedgewebview2)' } | Select-Object Name,ProcessId,CommandLine | ConvertTo-Json -Depth 3; Add-Type -AssemblyName System.Windows.Forms; Add-Type -AssemblyName System.Drawing; $bounds=[Windows.Forms.Screen]::PrimaryScreen.Bounds; $bitmap=[Drawing.Bitmap]::new($bounds.Width,$bounds.Height); $graphics=[Drawing.Graphics]::FromImage($bitmap); $graphics.CopyFromScreen($bounds.Location,[Drawing.Point]::Empty,$bounds.Size); $bitmap.Save('test-results/native/startup-desktop.png'); $graphics.Dispose(); $bitmap.Dispose()`,
      ],
      { encoding: 'utf8', windowsHide: true, timeout: 15000 },
    );
    writeFileSync(
      join(reports, 'processes.log'),
      diagnostic.stdout + diagnostic.stderr,
    );
    throw new Error(
      `Native WebView2 did not expose its CI debugging endpoint: ${lastConnectionError}`,
    );
  }
  let page;
  for (let attempt = 0; attempt < 80; attempt++) {
    page = browser
      .contexts()
      .flatMap((c) => c.pages())
      .find((p) => /tauri\.localhost|tauri:\/\//.test(p.url()));
    if (page) break;
    await new Promise((r) => setTimeout(r, 250));
  }
  if (!page) throw new Error('Production app page did not load.');
  await page
    .getByRole('heading', { name: 'Da YouTube. Al tuo spazio.' })
    .waitFor();
  await page.evaluate(() =>
    window.__TAURI_INTERNALS__.invoke('preview', { id: 'el3E4MbxRqQ' }),
  );
  let remote;
  for (let attempt = 0; attempt < 60; attempt++) {
    remote = browser
      .contexts()
      .flatMap((c) => c.pages())
      .find(
        (p) => p !== page && p.url().includes('youtube-nocookie.com/embed/'),
      );
    if (remote) break;
    await new Promise((r) => setTimeout(r, 250));
  }
  if (!remote) throw new Error('Preview webview did not open.');
  if (new URL(remote.url()).searchParams.get('autoplay') !== '0')
    throw new Error('Preview unexpectedly autoplays.');
  const privilege = await remote.evaluate(async () => {
    if (!window.__TAURI_INTERNALS__) return 'no-bridge';
    try {
      await window.__TAURI_INTERNALS__.invoke('snapshot', {});
      return 'unexpected-access';
    } catch {
      try {
        await window.__TAURI_INTERNALS__.invoke('plugin:fs|read_text_file', {
          path: 'C:\\Windows\\win.ini',
        });
        return 'unexpected-access';
      } catch {
        return 'denied';
      }
    }
  });
  if (privilege === 'unexpected-access')
    throw new Error('Remote preview can invoke native commands.');
  await remote.close();
  const session = await page.context().newCDPSession(page);
  const scales = [];
  for (const scale of [1, 1.5, 2]) {
    await session.send('Emulation.setDeviceMetricsOverride', {
      width: 1024,
      height: 700,
      deviceScaleFactor: scale,
      mobile: false,
    });
    await page.evaluate(
      () =>
        new Promise((resolve) =>
          requestAnimationFrame(() => requestAnimationFrame(resolve)),
        ),
    );
    const pixelRatio = await page.evaluate(() => window.devicePixelRatio);
    if (Math.abs(pixelRatio - scale) > 0.01)
      throw new Error(
        `Native rendering scale did not apply: expected ${scale}, got ${pixelRatio}`,
      );
    scales.push({ requested: scale, devicePixelRatio: pixelRatio });
    const button = page.getByRole('button', { name: 'Aggiungi alla coda' });
    await button.waitFor();
    const box = await button.boundingBox();
    if (!box || box.y + box.height > 700)
      throw new Error('Primary action outside the viewport.');
    await page.screenshot({
      path: join(reports, `native-add-${scale}.png`),
      fullPage: true,
    });
  }
  writeFileSync(
    join(reports, 'rendering-scales.json'),
    JSON.stringify(scales, null, 2),
  );
  await page.getByRole('button', { name: 'Impostazioni', exact: true }).click();
  const folder = join(reports, 'downloads');
  mkdirSync(folder, { recursive: true });
  await page.getByLabel('Cartella di download').fill(folder);
  await page.getByLabel('Download simultanei').fill('2');
  await page.getByRole('button', { name: 'Salva impostazioni' }).click();
  await page.getByText('Impostazioni salvate').waitFor();
  const state = await page.evaluate(() =>
    window.__TAURI_INTERNALS__.invoke('snapshot', {}),
  );
  if (
    state.settings.concurrent_jobs !== 2 ||
    state.settings.defaults.destination !== folder
  )
    throw new Error('Native settings roundtrip failed.');
  await page.screenshot({
    path: join(reports, 'native-settings.png'),
    fullPage: true,
  });
  await page.getByLabel('Aspetto').selectOption('light');
  await page.getByRole('button', { name: 'Salva impostazioni' }).click();
  await page.screenshot({
    path: join(reports, 'native-light.png'),
    fullPage: true,
  });
  await page.getByRole('button', { name: 'Coda download' }).click();
  await page.getByRole('heading', { name: 'La tua coda.' }).waitFor();
  await page.screenshot({
    path: join(reports, 'native-queue.png'),
    fullPage: true,
  });
  await page
    .evaluate(() => window.__TAURI_INTERNALS__.invoke('shutdown', {}))
    .catch(() => {});
  console.log(
    'Native installed UI, three rendering scales and persistent settings passed.',
  );
} finally {
  await browser?.close().catch(() => {});
  if (child.exitCode === null) child.kill();
  const removed = spawnSync(
    registry,
    ['delete', policy, '/v', 'Frame.exe', '/f'],
    { encoding: 'utf8', windowsHide: true },
  );
  if (removed.error || removed.status !== 0)
    throw new Error('Cannot remove isolated CI WebView2 debugging policy.');
}
