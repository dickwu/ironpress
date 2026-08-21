import { readFile } from 'node:fs/promises';

import initialize from './ironpress.js';

export * from './ironpress.js';

let initialization;

export default function init() {
  initialization ??= initializeNode();
  return initialization;
}

async function initializeNode() {
  try {
    const wasm = await readFile(new URL('./ironpress_bg.wasm', import.meta.url));
    return await initialize({ module_or_path: wasm });
  } catch (cause) {
    throw new Error(
      'Failed to initialize Ironpress WebAssembly in Node.js. ' +
        'The packaged ironpress_bg.wasm asset could not be loaded or instantiated.',
      { cause },
    );
  }
}
