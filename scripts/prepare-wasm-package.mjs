import { copyFile, readFile, writeFile } from 'node:fs/promises';

const packageDirectory = new URL('../pkg/', import.meta.url);
const manifestUrl = new URL('package.json', packageDirectory);
const manifest = JSON.parse(await readFile(manifestUrl, 'utf8'));

if (manifest.name !== 'ironpress' || manifest.type !== 'module') {
  throw new Error('wasm-pack did not generate the expected Ironpress ESM package');
}

await Promise.all(
  ['node.js', 'node.d.ts'].map((file) =>
    copyFile(
      new URL(`../bindings/wasm/${file}`, import.meta.url),
      new URL(file, packageDirectory),
    ),
  ),
);

manifest.files = [...new Set([...manifest.files, 'node.js', 'node.d.ts'])];
manifest.exports = {
  '.': {
    types: './ironpress.d.ts',
    import: './ironpress.js',
    default: './ironpress.js',
  },
  './node': {
    types: './node.d.ts',
    import: './node.js',
    default: './node.js',
  },
  './ironpress.js': {
    types: './ironpress.d.ts',
    import: './ironpress.js',
    default: './ironpress.js',
  },
  './ironpress.d.ts': './ironpress.d.ts',
  './ironpress_bg.wasm': './ironpress_bg.wasm',
  './package.json': './package.json',
};

await writeFile(manifestUrl, `${JSON.stringify(manifest, null, 2)}\n`);
