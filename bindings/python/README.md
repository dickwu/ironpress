# Ironpress for Python

Generate PDF bytes from HTML or Markdown without launching a browser.

## Installation

```bash
pip install ironpress
```

Ironpress publishes ABI3 wheels for CPython 3.8 and later on Linux, macOS, and
Windows.

## Usage

```python
import ironpress

pdf = ironpress.html_to_pdf("<h1>Hello</h1>")
with open("output.pdf", "wb") as output:
    output.write(pdf)
```

Use a converter when several options must compose or multiple documents share
the same policy:

```python
converter = ironpress.HtmlConverter()
converter.page_size("Letter")
converter.margin_sides(36, 54, 36, 54)
converter.header("Quarterly report")
converter.footer("Page {page} of {pages}")
converter.sanitize(True)

pdf = converter.convert("<h1>Results</h1>")
```

The converter supports page geometry, PDF and image quality, sanitization,
headers and footers, custom TTF fonts, and optional CJK or emoji packs. Native
Python also supports constrained local resources through `base_path()` and
`resource_root()`, plus direct file output.

See the [binding capability matrix](../README.md) for the contract shared with
Rust, Ruby, and WebAssembly.

## Font packs

Ironpress never downloads fallback fonts while rendering. Download the artifact
your application needs, verify it as part of deployment, then install its bytes:

```python
from pathlib import Path

converter = ironpress.HtmlConverter()
converter.add_font_pack(
    "cjk-jp",
    Path("ironpress-font-cjk-jp.ttf").read_bytes(),
)
pdf = converter.convert("<p lang='ja'>日本語</p>")
```

Valid pack names are `cjk-jp`, `cjk-kr`, `cjk-sc`, `cjk-tc`, and `emoji`.
