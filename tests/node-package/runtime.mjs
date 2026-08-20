import { readFile } from 'node:fs/promises';

import browserInit from 'ironpress';
import init, { HtmlConverter } from 'ironpress/node';

if (typeof browserInit !== 'function') {
  throw new Error('the browser package entry point no longer exports its initializer');
}

await init();

const converter = new HtmlConverter();
converter.pageSizeCustom(320, 480);
converter.addFontPack('cjk-jp', await readFile(process.argv[2]));

const pdf = Buffer.from(converter.htmlToPdf("<p lang='ja'>第</p>"));
converter.free();

if (!pdf.subarray(0, 4).equals(Buffer.from('%PDF'))) {
  throw new Error('the installed Node package did not produce a PDF');
}
if (!pdf.includes(Buffer.from('/MediaBox [0 0 320 480]'))) {
  throw new Error('the Node converter configuration did not reach the PDF');
}
if (!pdf.includes(Buffer.from('DroidSansFallback'))) {
  throw new Error('the caller-provided font pack did not reach the PDF');
}
