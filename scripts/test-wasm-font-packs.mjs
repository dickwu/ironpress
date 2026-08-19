import { readFile } from 'node:fs/promises';
import { join } from 'node:path';

import init, { HtmlConverter } from '../pkg/ironpress.js';

const wasm = await readFile(new URL('../pkg/ironpress_bg.wasm', import.meta.url));
await init({ module_or_path: wasm });

const artifactDirectory = process.argv[2];
const cases = artifactDirectory
  ? [
      ['cjk-jp', 'ironpress-font-cjk-jp.ttf', "<p lang='ja'>日本語</p>", 'NotoSansJP'],
      ['cjk-kr', 'ironpress-font-cjk-kr.ttf', "<p lang='ko'>안녕하세요</p>", 'NotoSansKR'],
      ['cjk-sc', 'ironpress-font-cjk-sc.ttf', "<p lang='zh-Hans'>简体中文</p>", 'NotoSansSC'],
      ['cjk-tc', 'ironpress-font-cjk-tc.ttf', "<p lang='zh-Hant'>繁體中文</p>", 'NotoSansTC'],
      ['emoji', 'ironpress-font-emoji.ttf', '<p>😀</p>', 'NotoEmoji'],
    ]
  : [
      [
        'cjk-jp',
        new URL('../tests/fonts/IronpressCjkVertical.ttf', import.meta.url),
        "<p lang='ja'>第</p>",
        'DroidSansFallback',
      ],
    ];

for (const [kind, source, html, expectedFont] of cases) {
  const converter = new HtmlConverter();
  const bytes = await readFile(artifactDirectory ? join(artifactDirectory, source) : source);
  converter.addFontPack(kind, bytes);

  const rawPdf = Buffer.from(converter.htmlToPdf(html)).toString('latin1');
  if (!rawPdf.includes(expectedFont)) {
    throw new Error(`${kind} did not embed ${expectedFont}`);
  }
  converter.free();
}
