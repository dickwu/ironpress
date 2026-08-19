# Ironpress font packs

Ironpress ships its Latin, Arabic, Hebrew, and common Unicode fonts in `core`.
These five optional packs add full regional CJK faces and monochrome emoji:

- `cjk-jp`: Japanese glyph forms
- `cjk-kr`: Korean glyph forms and Hangul
- `cjk-sc`: Simplified Chinese glyph forms
- `cjk-tc`: Traditional Chinese glyph forms
- `emoji`: monochrome outline emoji

The renderer never downloads fonts. Applications choose, fetch, and install
packs explicitly. This keeps the crate and base WASM module small and makes
network policy the host application's responsibility.

Each Ironpress GitHub release contains the matching `.ttf` artifacts and their
OFL license files. `sources.lock` pins the upstream revisions, input hashes,
FontTools transformation, and output hashes. Run this to reproduce them:

```sh
scripts/build-font-packs.sh
```

The four Noto Sans CJK variable fonts are instantiated at weight 400. Ironpress
currently consumes a static TrueType face and would otherwise select the
variable font's first master instead of its Regular instance.

## Browser and WASM

The stateful WASM converter accepts a downloaded pack as a `Uint8Array`:

```js
import init, { HtmlConverter } from 'ironpress';

await init();
const converter = new HtmlConverter();
const response = await fetch('/fonts/ironpress-font-cjk-jp.ttf');
converter.addFontPack('cjk-jp', new Uint8Array(await response.arrayBuffer()));
const pdf = converter.htmlToPdf("<p lang='ja'>日本語</p>");
```

Cache the downloaded bytes or the configured converter in the host application.
Adding a pack with the same name replaces that role for later conversions.
