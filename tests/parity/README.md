# Visual parity

This corpus renders small, adversarial HTML/CSS fixtures with ironpress and
compares them with committed browser oracle PDFs at 300 DPI. It is both a
regression gate and a defect inventory: a passing percentage never hides the
fixtures that need attention.

Chrome is not launched by the test. Reference generation is a separate,
explicit operation.

## Files

```text
tests/feature_parity.rs                integration-test entry point
tests/parity_support/                  comparator, gate, and report code
tests/parity/cases/<category>/<id>.html
tests/parity/oracles/<category>/<id>.pdf committed browser output
tests/parity/refs/<category>/<id>.png  generated, ignored report previews
tests/parity/manifest/<category>.json  fixture metadata
tests/parity/refs.lock                 fixture, PDF oracle, and provenance lock
tests/parity/baseline.json             explicitly reviewed regression baseline
tests/parity/report.json               always-current machine-readable report
tests/parity/REPORT.md                 always-current compact human report
tests/parity/reports/                  visual HTML report
```

## Run it

```bash
scripts/parity.sh
```

A full run always writes the current JSON, Markdown, and visual report, then
fails closed when:

- any fixture does not receive `PASS`, regardless of its support label;
- an authenticated reference is missing, stale, or renamed;
- a committed fixture disappears or its status gets worse;
- a newly added fixture is not `PASS`; or
- the committed baseline is missing or malformed.

Every candidate and oracle source is rendered with
`tests/parity/ua-pins.css` injected before its own styles. The zero-specificity
author rules pin HTML display roles, body margins, root typography, list/table
behavior, links, headings, and other browser-UA choices without preventing a
fixture from overriding them. Schema 6 of `refs.lock` authenticates that exact
stylesheet hash, so a baseline change forces real oracle regeneration. The
corpus audit also rejects an empty/symlinked baseline or a source without an
explicit `<head>` insertion point.

An already-failing raster is fingerprinted page by page. Any change to it needs
explicit baseline review; a rounded percentage cannot silently launder a moved
or newly shaped defect.

To inspect a small subset without touching the baseline:

```bash
PARITY_ONLY=invalid-justify-content cargo test --test feature_parity -- --nocapture
```

This is diagnostic only and deliberately exits nonzero after rendering. A
filtered or zero-match selection can never satisfy the full-corpus gate. Its
images are written under `target/parity-diagnostics/run-<pid>/`; it does not
overwrite the images or documents belonging to the last full report.

For PDF-level investigation, add `PARITY_KEEP_PDFS=1`; the filtered run then
retains its candidate PDFs under that diagnostic directory's `pdfs/` tree.

An intentional full baseline replacement must be explicit:

```bash
PARITY_UPDATE_BASELINE=1 scripts/parity.sh
```

That mode skips only the comparison with the old baseline. Corpus and reference
integrity still gate. Reviewed FAIL rows and their exact raster fingerprints may
enter the regression snapshot, but remain FAIL in the current report and keep
the full gate red regardless of support label. The snapshot prevents those
known failures from moving or worsening and prevents new failures from being
introduced. Update mode cannot be combined with `PARITY_ONLY`.

To intentionally regenerate one browser oracle without rewriting its category,
set its fixture id explicitly:

```bash
PARITY_FIXTURE=background-repeat-space-round FORCE=1 \
  scripts/parity-gen-refs.sh backgrounds-borders
```

The generator preserves every out-of-scope lock entry and verifies the selected
fixture exists before it writes `refs.lock`.

## Add an adversarial fixture

1. Add one deterministic, standalone document at
   `cases/<category>/<id>.html`. Use no network resources. Prefer solid shapes
   and explicit dimensions over text unless typography is the subject. Include
   an explicit `<head>`; the harness injects the authenticated UA baseline
   there before the fixture's author styles.
2. Give the fixture an explicit, content-sized
   `@page { size: ...; margin: 0 }`. This keeps both engines on the same small
   canvas and makes a defect occupy a meaningful part of the raster. Chrome
   fixtures using CSS-pixel page sizes must use multiples of 8 at 300 DPI; run
   `scripts/parity-normalize-page-sizes.py` to check or normalize them.
3. Add its manifest entry. The filename stem, category, and id must agree.
   Unknown manifest fields are rejected. The same documented visibility policy
   applies to every fixture; there are no fixture-specific thresholds.
4. Generate the missing reference, then authenticate the complete corpus:

   ```bash
   scripts/parity-gen-refs.sh <category>
   scripts/parity-gen-refs.sh --check
   ```

5. Run the targeted fixture first, fix the underlying engine behavior, then run
   the complete corpus. Support labels are descriptive only; no label can waive
   a human-visible difference.

A typical manifest entry is:

```json
{
  "id": "flexbox-invalid-justify-content-preserves-prior-value",
  "category": "flexbox",
  "feature": "justify-content",
  "subfeature": "invalid-declaration-discard",
  "description": "A later invalid declaration is discarded.",
  "file": "cases/flexbox/flexbox-invalid-justify-content-preserves-prior-value.html",
  "kind": "feature",
  "oracle": "chrome",
  "expected_support": "implemented",
  "depends_on": ["probe-fill-box", "probe-block-flow"]
}
```

`expected_support` is one of `implemented`, `partial`, or `unsupported`; it does
not affect the verdict or gate.
`oracle` is `chrome` or, for authenticated historical evidence only,
`weasyprint`. New and stale oracle PDFs are generated only through the pinned
Chromium Fontations/Foundation launcher. The generator refuses to create or
replace a non-Chromium oracle. A parity fixture without a real PDF oracle is
rejected.

## Verdicts

The comparator classifies every non-identical pixel from the two values at that
coordinate: missing content, extra content, or colour error. Both PDFs use the
exact same `pdftoppm` executable and arguments, so a raster difference is
evidence that the PDFs differ, not evidence of different rasterizers.

The exact unequal-RGBA count is always retained numerically. The verdict and
full-page diff additionally apply one fixed same-coordinate human-visibility
policy: a pixel is semantically equal when every RGB channel differs by less
than 1%; only pixels above that floor are painted in the diff. The complete
page may have no more than 1% above-floor pixels, and an authored-scale shape,
span, recolour, missing mark, or extra mark can still fail below that aggregate
ceiling. Paper and colour ΔE2000 checks provide the remaining perceptual colour
classification. The comparator never translates, registers, crops-to-fit,
filters, resamples, replaces content, or uses fixture-specific thresholds.

| status | meaning |
|---|---|
| `PASS` | dimensions match and any remaining difference is below the fixed visibility policy |
| `FAIL` | visible pixel difference, render error, or dimension mismatch |

The headline percentage is an unweighted summary (`PASS=1`, `FAIL=0`). Every
fixture counts equally. Each fixture reports both the above-floor percentage
shown in its diff and the exact raw unequal-RGBA percentage; read those and the
needs-attention table first.

## Reference integrity

`refs.lock` binds each id to its complete manifest metadata, fixture bytes,
category/path, oracle PDF hash, renderer version, bundled fonts, and generator.
PNGs are deliberately excluded:
the test rasterizes the committed oracle PDF and candidate PDF through one
discovered runtime `pdftoppm` executable with one option set.

Oracle regeneration requires Chromium and Poppler for PDF validation. The
launcher forces `FontationsFontBackend` and `FontationsLinuxSystemFonts`, then
verifies the pinned x-height probe before generation. Existing authenticated
legacy PDFs are left untouched; a stale legacy reference fails instead of
being silently regenerated by a different renderer. Other existing PDFs are
left untouched unless `--force` is supplied; use force only when oracle output
is intentionally being replaced.
