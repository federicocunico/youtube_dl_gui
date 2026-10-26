import { createHash } from 'node:crypto';
import {
  readdirSync,
  copyFileSync,
  mkdirSync,
  readFileSync,
  writeFileSync,
} from 'node:fs';
import { join, resolve } from 'node:path';
const source = resolve('src-tauri/target/release/bundle/nsis');
const output = resolve('artifacts');
mkdirSync(output, { recursive: true });
const installers = readdirSync(source).filter((name) =>
  name.endsWith('-setup.exe'),
);
if (installers.length !== 1) throw new Error('Expected one Windows installer.');
const files = [
  ...installers,
  'runtime-lock.json',
  'engine-requirements.txt',
  'THIRD-PARTY-NOTICES.md',
  'inventory.json',
];
copyFileSync(join(source, installers[0]), join(output, installers[0]));
for (const name of files.slice(1))
  copyFileSync(
    name === 'inventory.json'
      ? join('resources/runtime', name)
      : join('resources', name),
    join(output, name),
  );
writeFileSync(
  join(output, 'SIGNATURE.txt'),
  'This build is unsigned. No Authenticode signature is claimed.\n',
);
writeFileSync(
  join(output, 'SHA256SUMS.txt'),
  [...files, 'SIGNATURE.txt']
    .map(
      (name) =>
        `${createHash('sha256')
          .update(readFileSync(join(output, name)))
          .digest('hex')}  ${name}`,
    )
    .join('\n') + '\n',
);
console.log(join(output, installers[0]));
