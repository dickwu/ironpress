# Language bindings

Ironpress exposes one rendering engine through Rust, Python, Ruby, and
WebAssembly. Each binding keeps the conventions of its language while sharing
the portable converter contract below.

## Capability matrix

| Capability | Rust | Python | Ruby | WebAssembly |
|---|:---:|:---:|:---:|:---:|
| HTML and Markdown to PDF bytes | Yes | Yes | Yes | Yes |
| Reusable configured converter | Yes | Yes | Yes | Yes |
| Page size and margins | Yes | Yes | Yes | Yes |
| PDF and raster quality controls | Yes | Yes | Yes | Yes |
| Sanitization | Yes | Yes | Yes | Yes |
| Headers and footers | Yes | Yes | Yes | Yes |
| Custom TTF fonts | Yes | Yes | Yes | Yes |
| Optional CJK and emoji packs | Yes | Yes | Yes | Yes |
| Local resource boundary | Yes | Yes | Yes | No |
| Direct file output | Yes | Yes | Yes | No |
| Streaming writer and async API | Yes | No | No | No |
| Remote HTTP resources | Opt-in | No | No | No |

WebAssembly receives font and document bytes from the host application. It does
not read local paths. Remote resource loading remains available only through the
Rust crate's opt-in `remote` feature so language packages do not silently gain
network access.

## Published artifacts

| Runtime | Package | Supported artifact contract |
|---|---|---|
| Rust | [`ironpress`](https://crates.io/crates/ironpress) | Source crate, Rust 1.88+ |
| Python | [`ironpress`](https://pypi.org/project/ironpress/) | CPython 3.8+ ABI3 wheels for Linux, macOS, and Windows |
| Ruby | [`ironpress`](https://rubygems.org/gems/ironpress) | Ruby 3.0+ source gem and native gems for Linux, macOS, and Windows |
| WebAssembly | [`ironpress`](https://www.npmjs.com/package/ironpress) | Browser-targeted ES module and WebAssembly binary |

All four packages use the same Ironpress version. CI builds and installs the
Python and Ruby artifacts before a release can publish them, and exercises the
generated npm package through Node.

See the runtime guides for language-specific examples:

- [Python](python/README.md)
- [Ruby](ruby/README.md)
- [WebAssembly and playground](https://github.com/gastongouron/ironpress/wiki/WASM-Playground)
