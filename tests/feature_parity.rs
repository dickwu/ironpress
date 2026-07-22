//! Feature-parity integration test entry point.
//!
//! Run with: `cargo test --test feature_parity -- --nocapture`
//!
//! The engine renders every fixture under `tests/parity/cases/**` in-process
//! through the `ironpress` library, then rasterizes both that PDF and the
//! committed browser-oracle PDF through the same `pdftoppm` executable with the
//! same arguments. It records identical page dimensions and raw RGBA bytes as
//! same-coordinate diagnostic evidence, then applies the documented
//! human-visibility parity policy; it writes the current report and enforces
//! the gate against `tests/parity/baseline.json`.
//!
//! See `tests/parity/README.md` for the full workflow.

mod parity_support;

#[test]
fn feature_parity() -> Result<(), String> {
    parity_support::run()
}
