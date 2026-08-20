# Ironpress for Ruby

Generate PDF bytes from HTML or Markdown without launching a browser.

## Installation

```ruby
gem "ironpress"
```

Ironpress supports Ruby 3.0 and later. Releases include a source gem and native
gems for Linux, macOS, and Windows.

## Usage

```ruby
require "ironpress"

pdf = Ironpress.html_to_pdf("<h1>Hello</h1>")
File.binwrite("output.pdf", pdf)
```

Configuration methods return the converter, so policies can be composed in an
idiomatic chain:

```ruby
converter = Ironpress::HtmlConverter.new
  .page_size("Letter")
  .margin_sides(36, 54, 36, 54)
  .header("Quarterly report")
  .footer("Page {page} of {pages}")
  .sanitize(true)

pdf = converter.convert("<h1>Results</h1>")
```

The converter supports page geometry, PDF and image quality, sanitization,
headers and footers, custom TTF fonts, and optional CJK or emoji packs. Native
Ruby also supports constrained local resources through `base_path` and
`resource_root`, plus direct file output.

See the [binding capability matrix](../README.md) for the contract shared with
Rust, Python, and WebAssembly.

## Font packs

Ironpress never downloads fallback fonts while rendering. Download the artifact
your application needs, verify it as part of deployment, then install its bytes:

```ruby
converter = Ironpress::HtmlConverter.new.add_font_pack(
  "cjk-jp",
  File.binread("ironpress-font-cjk-jp.ttf")
)
pdf = converter.convert("<p lang='ja'>日本語</p>")
```

Valid pack names are `cjk-jp`, `cjk-kr`, `cjk-sc`, `cjk-tc`, and `emoji`.
