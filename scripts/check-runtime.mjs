import { createHash } from 'node:crypto';
import { readFileSync, existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { join, isAbsolute } from 'node:path';
import { spawnSync } from 'node:child_process';

const root = fileURLToPath(new URL('../', import.meta.url));
const runtime = join(root, 'resources', 'runtime');
const lock = JSON.parse(
  readFileSync(join(root, 'resources', 'runtime-lock.json'), 'utf8'),
);
const inventory = JSON.parse(
  readFileSync(join(runtime, 'inventory.json'), 'utf8'),
);
for (const [name, binary] of Object.entries(inventory.binaries)) {
  const path = join(runtime, binary.path);
  if (!isAbsolute(path) || !existsSync(path))
    throw new Error(`Missing bundled ${name}`);
  const hash = createHash('sha256').update(readFileSync(path)).digest('hex');
  if (hash !== binary.sha256) throw new Error(`Changed ${name}`);
  const result = spawnSync(path, [name === 'deno' ? '--version' : '-version'], {
    encoding: 'utf8',
    windowsHide: true,
    env: { ...process.env, PATH: '' },
  });
  if (result.error || result.status !== 0)
    throw new Error(`Cannot run private ${name}: ${result.stderr}`);
  console.log(result.stdout.split('\n')[0]);
}
const python = join(runtime, inventory.pythonPath);
const code =
  "import sys, pip, yt_dlp.version, packaging; assert yt_dlp.version.__version__ == sys.argv[1]; print(sys.executable); print('yt-dlp ' + yt_dlp.version.__version__)";
const result = spawnSync(python, ['-I', '-c', code, lock.engine], {
  encoding: 'utf8',
  windowsHide: true,
  env: { ...process.env, PATH: '', PYTHONPATH: '', PYTHONHOME: '' },
});
if (result.error || result.status !== 0)
  throw new Error(result.stderr || String(result.error));
if (!result.stdout.toLowerCase().includes(python.toLowerCase()))
  throw new Error('Unexpected Python interpreter');
for (const asset of [lock.python, lock.ffmpeg, lock.deno])
  if (!/^[a-f0-9]{64}$/.test(asset.sha256))
    throw new Error('Missing upstream digest');
console.log(result.stdout.trim());
console.log('Private runtime verified with empty PATH.');
