import { createHash } from 'node:crypto';
import {
  existsSync,
  mkdirSync,
  readFileSync,
  writeFileSync,
  readdirSync,
  copyFileSync,
  statSync,
} from 'node:fs';
import { resolve, join, basename, relative, isAbsolute } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const root = fileURLToPath(new URL('../', import.meta.url));
const resources = join(root, 'resources');
const cache = join(root, '.runtime-cache');
const runtime = join(resources, 'runtime');
const lockPath = join(resources, 'runtime-lock.json');
const resolving = process.argv.includes('--resolve');
if (process.platform !== 'win32')
  throw new Error('Runtime provisioning requires Windows x64');
if (process.arch !== 'x64') throw new Error('Only x64 is supported');
for (const path of [
  cache,
  runtime,
  join(runtime, 'bin'),
  join(runtime, 'licenses'),
])
  mkdirSync(path, { recursive: true });

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: root,
    encoding: 'utf8',
    windowsHide: true,
    maxBuffer: 16 * 1024 * 1024,
    ...options,
  });
  if (result.error || result.status !== 0)
    throw new Error(
      `${basename(command)} failed: ${result.error?.message ?? result.stderr ?? result.stdout}`,
    );
  return result.stdout.trim();
}
async function json(url) {
  const response = await fetch(url, {
    headers: { 'User-Agent': 'Frame-runtime-provisioner' },
    signal: AbortSignal.timeout(60000),
  });
  if (!response.ok) throw new Error(`${url}: HTTP ${response.status}`);
  return response.json();
}
function digest(bytes) {
  return createHash('sha256').update(bytes).digest('hex');
}
async function download(asset) {
  const file = join(cache, `${asset.sha256 ?? 'resolve'}-${asset.name}`);
  if (
    existsSync(file) &&
    asset.sha256 &&
    digest(readFileSync(file)) === asset.sha256
  )
    return file;
  const response = await fetch(asset.url, {
    signal: AbortSignal.timeout(300000),
  });
  if (!response.ok)
    throw new Error(`Download ${asset.name}: HTTP ${response.status}`);
  const bytes = Buffer.from(await response.arrayBuffer());
  const hash = digest(bytes);
  if (asset.sha256 && hash !== asset.sha256)
    throw new Error(`SHA-256 mismatch: ${asset.name}`);
  asset.sha256 = hash;
  writeFileSync(file, bytes);
  return file;
}
async function githubAsset(repo, matcher, latest = false) {
  const data = await json(
    `https://api.github.com/repos/${repo}/releases${latest ? '/latest' : '?per_page=100'}`,
  );
  const releases = Array.isArray(data) ? data : [data];
  for (const release of releases) {
    if (
      release.draft ||
      (repo === 'BtbN/FFmpeg-Builds' && release.tag_name === 'latest')
    )
      continue;
    if (repo === 'BtbN/FFmpeg-Builds') {
      const currentMonth = new Date().toISOString().slice(0, 7);
      const releasedMonth = release.tag_name.match(
        /^autobuild-(\d{4}-\d{2})-/,
      )?.[1];
      // Older month-end releases have a two-year retention policy upstream.
      if (!releasedMonth || releasedMonth >= currentMonth) continue;
    }
    const asset = release.assets.find((a) => matcher.test(a.name));
    if (asset)
      return {
        version: release.tag_name,
        name: asset.name,
        url: asset.browser_download_url,
        sha256: asset.digest?.replace(/^sha256:/, ''),
        source: release.html_url,
      };
  }
  throw new Error(`No matching asset in ${repo}`);
}
function extract(archive, target) {
  mkdirSync(target, { recursive: true });
  const listing = run('tar.exe', ['-tf', archive]);
  for (const name of listing.split(/\r?\n/)) {
    const destination = resolve(target, name);
    const rel = relative(target, destination);
    if (
      isAbsolute(rel) ||
      rel.startsWith('..') ||
      /^[A-Za-z]:/.test(name) ||
      name.startsWith('/')
    )
      throw new Error(`Unsafe archive path: ${name}`);
  }
  run('tar.exe', ['-xf', archive, '-C', target]);
}
function walk(path) {
  return readdirSync(path, { withFileTypes: true }).flatMap((e) =>
    e.isDirectory() ? walk(join(path, e.name)) : [join(path, e.name)],
  );
}

let lock;
if (resolving) {
  const [python, ffmpeg, deno, pypi] = await Promise.all([
    githubAsset(
      'astral-sh/python-build-standalone',
      /^cpython-3\.13\..*-x86_64-pc-windows-msvc-install_only_stripped\.tar\.gz$/,
      true,
    ),
    githubAsset('BtbN/FFmpeg-Builds', /^ffmpeg-N-.*win64-lgpl-shared\.zip$/),
    githubAsset('denoland/deno', /^deno-x86_64-pc-windows-msvc\.zip$/, true),
    json('https://pypi.org/pypi/yt-dlp/json'),
  ]);
  lock = {
    schema: 1,
    target: 'windows-x64',
    python,
    ffmpeg,
    deno,
    engine: pypi.info.version,
    licenses: [
      {
        name: 'deno-LICENSE.md',
        url: `https://raw.githubusercontent.com/denoland/deno/${deno.version}/LICENSE.md`,
      },
    ],
  };
} else {
  if (!existsSync(lockPath))
    throw new Error(
      'Missing runtime-lock.json. Maintainers must run npm run runtime -- --resolve first.',
    );
  lock = JSON.parse(readFileSync(lockPath, 'utf8'));
}

for (const key of ['python', 'ffmpeg', 'deno']) {
  const asset = lock[key];
  console.log(`${key}: ${asset.version} (${asset.name})`);
  const archive = await download(asset);
  const target =
    key === 'python' ? runtime : join(cache, `extract-${key}-${asset.sha256}`);
  extract(archive, target);
  if (key !== 'python') {
    for (const file of walk(target)) {
      if (
        /\.(exe|dll)$/i.test(file) &&
        (key === 'ffmpeg' || basename(file) === 'deno.exe')
      )
        copyFileSync(file, join(runtime, 'bin', basename(file)));
      if (/licen[cs]e|copying|notice/i.test(basename(file)))
        copyFileSync(
          file,
          join(runtime, 'licenses', `${key}-${basename(file)}`),
        );
    }
  }
}
for (const license of lock.licenses ?? []) {
  const file = await download(license);
  copyFileSync(file, join(runtime, 'licenses', license.name));
}
const pythonExe = join(runtime, 'python', 'python.exe');
const env = { ...process.env, PYTHONNOUSERSITE: '1', PIP_CONFIG_FILE: 'NUL' };
delete env.PYTHONPATH;
delete env.PYTHONHOME;
const pythonVersion = run(pythonExe, ['-I', '--version']);
run(pythonExe, ['-I', '-m', 'pip', '--isolated', '--version'], { env });
const requirements = join(resources, 'engine-requirements.txt');
if (resolving) {
  const report = join(cache, 'baseline-report.json');
  run(
    pythonExe,
    [
      '-I',
      '-m',
      'pip',
      '--isolated',
      'install',
      '--dry-run',
      '--ignore-installed',
      '--only-binary=:all:',
      '--index-url',
      'https://pypi.org/simple',
      '--report',
      report,
      `yt-dlp[default]==${lock.engine}`,
      'packaging',
    ],
    { env },
  );
  const packages = JSON.parse(readFileSync(report, 'utf8')).install;
  lock.packages = packages.map((p) => {
    const url = new URL(p.download_info.url);
    if (
      url.protocol !== 'https:' ||
      url.hostname !== 'files.pythonhosted.org' ||
      !url.pathname.endsWith('.whl')
    )
      throw new Error('Untrusted package origin');
    return {
      name: p.metadata.name,
      version: p.metadata.version,
      sha256: p.download_info.archive_info.hashes.sha256,
    };
  });
  writeFileSync(
    requirements,
    lock.packages
      .map((p) => `${p.name}==${p.version} --hash=sha256:${p.sha256}`)
      .join('\n') + '\n',
  );
  writeFileSync(lockPath, JSON.stringify(lock, null, 2) + '\n');
}
run(
  pythonExe,
  [
    '-I',
    '-m',
    'pip',
    '--isolated',
    'install',
    '--require-hashes',
    '--only-binary=:all:',
    '--index-url',
    'https://pypi.org/simple',
    '-r',
    requirements,
  ],
  { env },
);
const binaries = {};
for (const name of ['ffmpeg', 'ffprobe', 'deno']) {
  const file = join(runtime, 'bin', `${name}.exe`);
  if (!existsSync(file)) throw new Error(`Missing bundled binary ${name}`);
  const version = run(file, [name === 'deno' ? '--version' : '-version']).split(
    '\n',
  )[0];
  binaries[name] = {
    path: `bin/${name}.exe`,
    sha256: digest(readFileSync(file)),
    version,
    bytes: statSync(file).size,
  };
}
const inventory = {
  ...lock,
  pythonVersion,
  pythonPath: 'python/python.exe',
  binaries,
  signature: 'unsigned',
};
writeFileSync(
  join(runtime, 'inventory.json'),
  JSON.stringify(inventory, null, 2) + '\n',
);
writeFileSync(
  join(resources, 'THIRD-PARTY-NOTICES.md'),
  `# Third-party runtime components\n\nThis distribution includes the following upstream components. See the bundled licenses directory and Python package metadata for complete licenses and source information.\n\n- CPython / Python Standalone Builds: PSF and bundled dependencies. ${lock.python.source}\n- FFmpeg LGPL shared build: LGPL and bundled dependencies. Source and build configuration: ${lock.ffmpeg.source}\n- Deno: MIT and bundled dependencies. ${lock.deno.source}\n- yt-dlp: Unlicense, with dependency licenses in python/Lib/site-packages. https://github.com/yt-dlp/yt-dlp\n\nExact package versions and hashes are recorded in runtime-lock.json and runtime/inventory.json. No Authenticode signature is claimed.\n`,
);
console.log(`Runtime verified: ${pythonVersion}, yt-dlp ${lock.engine}`);
