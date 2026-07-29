
<div align="center">

<img width="188" alt="4" src="https://github.com/user-attachments/assets/e8b569e6-e74c-4c0f-9e84-05cf37fae3ae" />

# Ironpress

Pure rust HTML/CSS/Markdown to PDF converter. No browser, no system dependencies.


[![Crates.io](https://img.shields.io/crates/v/ironpress.svg)](https://crates.io/crates/ironpress)
[![PyPI](https://img.shields.io/pypi/v/ironpress.svg)](https://pypi.org/project/ironpress/)
[![Gem](https://img.shields.io/gem/v/ironpress.svg)](https://rubygems.org/gems/ironpress)
[![npm](https://img.shields.io/npm/v/ironpress.svg)](https://www.npmjs.com/package/ironpress)
[![docs.rs](https://docs.rs/ironpress/badge.svg)](https://docs.rs/ironpress)
[![CI](https://github.com/gastongouron/ironpress/actions/workflows/ci.yml/badge.svg)](https://github.com/gastongouron/ironpress/actions)
[![codecov](https://codecov.io/gh/gastongouron/ironpress/branch/main/graph/badge.svg?token=w36XIAwRxG)](https://codecov.io/gh/gastongouron/ironpress)
[![deps.rs](https://deps.rs/repo/github/gastongouron/ironpress/status.svg)](https://deps.rs/repo/github/gastongouron/ironpress)
[![MSRV](https://img.shields.io/badge/MSRV-1.85-blue.svg)](https://www.rust-lang.org)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Downloads](https://img.shields.io/crates/d/ironpress.svg)](https://crates.io/crates/ironpress)
[![WASM](https://img.shields.io/badge/wasm-ready-blueviolet.svg)](../../wiki/WASM-Playground)
[![Playground](https://img.shields.io/badge/try_it-playground-blueviolet.svg)](https://gastongouron.github.io/ironpress/)
[![Parity report](https://img.shields.io/badge/parity-report-ff69b4.svg)](https://htmlpreview.github.io/?https://github.com/Wicpar/ironpress/blob/parity-report/tests/parity/reports/index.html)

**[Try it in your browser](https://gastongouron.github.io/ironpress/)** | **[HTML parity report](https://htmlpreview.github.io/?https://github.com/Wicpar/ironpress/blob/parity-report/tests/parity/reports/index.html)** | **[Wiki](../../wiki)**

</div>


## Performance

Criterion measures complete, in-process conversion to PDF bytes:

| Document | Representative median | Approx. conversions/sec |
|----------|----------------------:|------------------------:|
| Simple HTML (`<h1>` + `<p>`) | **0.93 ms** | 1,080 |
| Styled HTML (CSS, lists, links) | **3.5 ms** | 285 |
| Table (5 rows, styled headers) | **5.9 ms** | 170 |
| Markdown (headings, code, lists) | **7.0 ms** | 143 |
| Full report (tables, flex, progress bars) | **15.9 ms** | 63 |
| Full report + header/footer | **15.9 ms** | 63 |
| 200 CJK text-emphasis spans | **310 ms** | 3.2 |

Measured on macOS ARM (Apple M2) with Rust 1.94.0 at commit
`600aadb`. The benchmark profile uses `opt-level=3`, fat LTO, and one codegen
unit. Results depend on hardware and document content; conversions/sec is not a
page-throughput claim. Reproduce the measurements with:

```bash
cargo bench --bench conversion
```

## Quick start

```rust
use ironpress::html_to_pdf;

let pdf = html_to_pdf("<h1>Hello</h1><p>World</p>").unwrap();
std::fs::write("output.pdf", pdf).unwrap();
```

```rust
let pdf = ironpress::markdown_to_pdf("# Hello\n\nWorld").unwrap();
```

## CLI

```bash
cargo install ironpress

ironpress input.html output.pdf
ironpress document.md output.pdf
ironpress --page-size letter --landscape --margin 54 input.html output.pdf
ironpress --header "Report" --footer "Page {page} of {pages}" input.html output.pdf
echo '<h1>Hello</h1>' | ironpress --stdin output.pdf
```

## Builder API

```rust
use ironpress::{HtmlConverter, Margin, PageSize, RasterQuality};

let pdf = HtmlConverter::new()
    .page_size(PageSize::LETTER)
    .margin(Margin::uniform(54.0))
    .raster_quality(RasterQuality {
        background_dpi: 144.0,
        ..RasterQuality::default()
    })
    .header("My Document")
    .footer("Page {page} of {pages}")
    .convert("<h1>Custom page</h1>")
    .unwrap();
```

`RasterQuality` keeps source-image, filter, and flattened-background resolution
in one physical-DPI policy. Its default preserves sharp source/filter output
while using 192 DPI for flattened synthetic backgrounds; lowering one field
does not change page geometry. The CLI exposes the same controls through
`--image-dpi`, `--filter-dpi`, and `--background-raster-dpi`.

## Features at a glance

| Area | Highlights | Details |
|------|-----------|---------|
| **HTML** | 50+ elements: headings, tables, lists, forms, media, `<img>`, inline `<svg>` | [Layout Engine](../../wiki/Layout-Engine) |
| **CSS** | Flexbox, grid, multi-column, `calc()`, variables, `@media`, `@page`, `@font-face` | [CSS Support](../../wiki/CSS-Support) |
| **Fonts** | Base-14 PDF fonts, custom TTF embedding with subsetting, system font discovery, Unicode/CJK fallback | [Font System](../../wiki/Font-System) |
| **Math** | LaTeX via `$...$` / `$$...$$`: fractions, roots, matrices, Greek, operators | [Math Engine](../../wiki/Math-Engine) |
| **SVG** | Vector rendering: path, shapes, gradients, transforms, clip paths, `viewBox` | [Layout Engine](../../wiki/Layout-Engine) |
| **Images** | JPEG + PNG, data URIs, local files, remote URLs (`remote` feature) | [Architecture](../../wiki/Architecture) |
| **PDF** | PDF 1.4, bookmarks, link annotations, headers/footers, gradients, streaming output | [PDF Rendering](../../wiki/PDF-Rendering) |
| **WASM** | `npm install ironpress` - runs 100% client-side in the browser | [WASM & Playground](../../wiki/WASM-Playground) |
| **Testing** | 3500+ unit tests, property-based tests, 6 fuzz targets, 1,662-fixture parity corpus | [Testing Strategy](../../wiki/Testing-Strategy) |

## Custom fonts

```rust
let pdf = HtmlConverter::new()
    .add_font("Inter", std::fs::read("Inter.ttf").unwrap())
    .convert(r#"<p style="font-family: Inter">Shaped with HarfBuzz</p>"#)
    .unwrap();
```

Fonts are shaped with [rustybuzz](https://crates.io/crates/rustybuzz), subset to used glyphs only, and embedded as CIDFontType2. Characters outside WinAnsi (CJK, Arabic, emoji) are rendered via automatic Unicode font fallback. See [Font System](../../wiki/Font-System).

## Math

```markdown
The equation $E = mc^2$ is famous.

$$\sum_{k=1}^{n} k = \frac{n(n+1)}{2}$$
```

Full LaTeX support: fractions, roots, matrices, Greek letters, operators, delimiters, accents. See [Math Engine](../../wiki/Math-Engine).

## Python / Ruby

```bash
pip install ironpress
```

```python
import ironpress
pdf = ironpress.html_to_pdf("<h1>Hello</h1>")
```

```bash
gem install ironpress
```

```ruby
require "ironpress"
pdf = Ironpress.html_to_pdf("<h1>Hello</h1>")
```

## WASM

```bash
npm install ironpress
```

```javascript
import init, { htmlToPdf, markdownToPdf } from 'ironpress';
await init();

const pdf = htmlToPdf('<h1>Hello</h1>');
const blob = new Blob([pdf], { type: 'application/pdf' });
```

See [WASM & Playground](../../wiki/WASM-Playground).

## Security

HTML is sanitized by default: `<script>`, `<iframe>`, event handlers, and `javascript:` URLs are stripped. Resources are sandboxed (local-only by default, 10 MB cap). SVG sanitizer strips dangerous elements. PNG decompression capped at 50 MB. Disable with `.sanitize(false)` if you trust the input.

## How it works

```
HTML/Markdown → Sanitize → Parse (html5ever) → Style cascade → Layout engine → PDF 1.4
```

See [Architecture](../../wiki/Architecture) for the full pipeline.

## Visual parity harness

`tests/parity/` is an adversarial HTML/CSS corpus with one focused fixture per
feature, value, or interaction. Ironpress produces the candidate PDF; the
declared oracle renderer's PDF is committed. At test time both PDFs go through the
same discovered `pdftoppm` executable with the same 300 DPI arguments. A fixture
passes only when a fixed, same-coordinate human-visibility policy finds no
visible defect. Every raw RGBA difference remains in the evidence; the harness
never translates, registers, or fixture-tunes either raster.

```bash
scripts/parity.sh                       # run the complete exact parity gate
scripts/parity-gen-refs.sh <category>   # regenerate oracle PDFs explicitly
scripts/parity-gen-refs.sh --check      # authenticate the complete corpus
```

- **Run it:** `scripts/parity.sh` supplies a fresh invocation identity, renders
  every fixture in-process, rasterizes candidate and oracle PDFs symmetrically,
  and verifies that JSON, Markdown, and HTML all belong to that invocation.
- **Read it:** the [HTML parity report](https://htmlpreview.github.io/?https://github.com/Wicpar/ironpress/blob/parity-report/tests/parity/reports/index.html)
  provides the complete visual evidence; `tests/parity/REPORT.md` is the compact,
  problem-first summary, and `tests/parity/report.json` contains the complete
  machine result.
- **Oracles:** committed PDFs are the source of truth. Oracle-preview, candidate,
  and diff PNGs are generated report evidence and are intentionally ignored.
  `refs.lock` authenticates each fixture, oracle PDF, manifest entry, renderer,
  fonts, and generator provenance. Every future oracle PDF is generated only by
  the pinned Chromium Fontations/Foundation launcher; authenticated historical
  non-Chromium PDFs are evidence-only and cannot be regenerated.
- **Baseline:** `tests/parity/baseline.json` is a separately reviewed regression
  snapshot. Updating it is explicit; retained FAILs remain current-health
  failures while their exact rasters become protected against movement or
  worsening.
- **CI:** `.github/workflows/parity.yml` runs the same browser-free gate, checks
  `refs.lock`, and uploads the current report and evidence even when defects make
  the gate fail.

## License

MIT
