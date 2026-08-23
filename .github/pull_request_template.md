## Summary

<!-- Explain what this changes and why. Keep the pull request focused. -->

## Related issue

<!-- Link the issue with "Closes #123" when applicable. -->

## Behavioral source

<!--
For rendering changes, link the applicable specification, verified oracle, or
reproduced regression that establishes the expected behavior.
-->

## Verification

<!-- List the exact commands run and their results. -->

- [ ] Tests cover the changed behavior
- [ ] `cargo fmt --check`
- [ ] `cargo clippy -- -D warnings`
- [ ] `cargo test`

## Visual parity

- [ ] This change does not affect rendered output
- [ ] Relevant parity fixtures and reports were updated
- [ ] Any new oracle PDF was generated with the pinned Chromium launcher

## Bindings and documentation

- [ ] The C, .NET, Java, Python, Ruby, and WebAssembly impact was considered
- [ ] Public API or behavior changes are documented
- [ ] User-visible changes have a changelog entry

## Final checks

- [ ] The change is focused and contains no unrelated cleanup
- [ ] No secrets, generated scratch files, or local artifacts are included
- [ ] I agree to follow the [Ironpress Code of Conduct](https://github.com/gastongouron/ironpress/blob/main/CODE_OF_CONDUCT.md)
