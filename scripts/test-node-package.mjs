import { execFile as execFileCallback } from 'node:child_process';
import {
  copyFile,
  mkdtemp,
  readFile,
  rename,
  rm,
  writeFile,
} from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';

const execFile = promisify(execFileCallback);
const packageDirectory = fileURLToPath(new URL('../pkg/', import.meta.url));
const fixtures = new URL('../tests/node-package/', import.meta.url);
const fontPack = fileURLToPath(
  new URL('../tests/fonts/IronpressCjkVertical.ttf', import.meta.url),
);
const cargoManifest = await readFile(
  new URL('../Cargo.toml', import.meta.url),
  'utf8',
);
const crateVersion = cargoManifest.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
if (!crateVersion) {
  throw new Error('could not read the Ironpress crate version');
}
const projectDirectory = await mkdtemp(join(tmpdir(), 'ironpress-node-package-'));

async function run(command, arguments_) {
  return execFile(command, arguments_, {
    cwd: projectDirectory,
    maxBuffer: 10 * 1024 * 1024,
  });
}

try {
  const { stdout } = await run('npm', [
    'pack',
    packageDirectory,
    '--json',
    '--pack-destination',
    projectDirectory,
  ]);
  const [{ filename }] = JSON.parse(stdout);
  const tarball = join(projectDirectory, filename);

  await writeFile(
    join(projectDirectory, 'package.json'),
    `${JSON.stringify({ private: true, type: 'module' }, null, 2)}\n`,
  );
  await Promise.all(
    ['runtime.mjs', 'initialization-error.mjs', 'types.ts', 'tsconfig.json'].map(
      (fixture) =>
        copyFile(new URL(fixture, fixtures), join(projectDirectory, fixture)),
    ),
  );

  await run('npm', [
    'install',
    '--ignore-scripts',
    '--no-audit',
    '--no-fund',
    tarball,
    'typescript@7.0.2',
  ]);
  await run(process.execPath, ['runtime.mjs', fontPack]);
  await run(process.execPath, [
    'node_modules/typescript/bin/tsc',
    '--project',
    'tsconfig.json',
  ]);

  const manifest = JSON.parse(
    await readFile(
      join(projectDirectory, 'node_modules/ironpress/package.json'),
      'utf8',
    ),
  );
  if (manifest.version !== crateVersion) {
    throw new Error(
      `installed npm version ${manifest.version} does not match ${crateVersion}`,
    );
  }

  await rename(
    join(projectDirectory, 'node_modules/ironpress/ironpress_bg.wasm'),
    join(projectDirectory, 'node_modules/ironpress/ironpress_bg.wasm.missing'),
  );
  await run(process.execPath, ['initialization-error.mjs']);
} finally {
  await rm(projectDirectory, { recursive: true, force: true });
}
