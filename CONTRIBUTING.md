# Contributing to ironpress

Thanks for your interest in contributing!

## Getting Started

1. Fork the repository
2. Create a feature branch: `git checkout -b my-feature`
3. Make your changes
4. Run the checks:
   ```bash
   cargo fmt --check
   cargo clippy -- -D warnings
   cargo test
   ```
5. Commit and push your branch
6. Open a pull request against `main`

## Guidelines

- All code must pass `cargo fmt`, `cargo clippy -- -D warnings`, and `cargo test`
- Add tests for new functionality
- Keep PRs focused on a single change
- Follow existing code style and conventions

## Code comments

The codebase is intentionally low on comments. Follow these conventions:

- **`///` doc comments** on public (and most private) structs, enums, fields, and functions. This is the dominant style. Explain *why* when the code encodes a non-obvious spec rule or design choice — a bare *what* is not enough for layout/CSS logic.
- **`//!` module-level docs** only when a file has a meaningful architectural role worth describing. Not on every file.
- **`//` inline comments** sparingly, only to annotate non-trivial algorithmic steps.
- **No block comments** (`/* */`).
- **No `FIXME` or `HACK` markers.** `TODO:` is acceptable on `#[ignore]` tests.
- Let well-named identifiers speak for themselves. Don't narrate what the code does, document why it does it that way.

## Reporting Issues

Open an issue on GitHub with a clear description and, if possible, a minimal reproduction.
