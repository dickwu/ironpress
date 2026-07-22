# ironpress parity health

<!-- parity-invocation-id: e6ea7fb613a2090bbb8746f0a398188e -->

<!-- parity-report-json-sha256: 7c19d32f95344689314ba9d4bc19fc5e7dd1f7c27cf738d0686dfa2e26d48d7e -->

| health | verified visual parity | exact raster | visual-policy | FAIL | disputed refs | total |
|:------:|-----------------------:|-------------:|--------------:|-----:|--------------:|------:|
| **BROKEN** | 99.06% | 826 | 761 | 15 | 10 | 1612 |

**Needs attention: 15 failing fixture(s) · 10 disputed reference(s) · 1 integrity item(s).** PASS rule: a fixed, same-coordinate human-visibility policy is applied after both PDFs use the same pdftoppm executable and arguments. It never translates, registers, or fixture-tunes either image. Every raw RGBA difference remains reported.

Scope: 509 category/feature pairs · labels only: implemented 1575 · partial 37 · unsupported 0 · supported-family interactions 300/300 across 24 families.

**Raster audit: 826 exact PASSes · 761 visual-policy PASSes (max raw difference 69.93%; CSS-scale observation: balanced edge coverage 1 · CSS-scale observation: conserved sub-CSS coverage 28 · CSS-scale observation: one-sided sub-CSS outline coverage 32 · CSS-scale observation: predominant shared-outline coverage 14 · CSS-scale observation: stable same-coordinate outline phase 76 · CSS-scale observation: sub-CSS shared-colour coverage 268 · CSS-scale observation: sub-CSS shared-outline coverage 201 · raw policy 141).** Each visual-policy fixture card keeps its raw difference and policy basis.

## Integrity

| state | run | gate | pdftoppm | refs.lock identity | baseline | stale refs | ref mismatches |
|:-----:|-----|------|----------|--------------------|----------|-----------:|---------------:|
| **BROKEN** | complete | FAILED | OK | authenticated | MISSING/INVALID/INCOMPATIBLE | 0 | 0 |

### Gate result

**REGRESSION — FAILED.** parity integrity gate FAILED (15 issue(s)):

## Failure triage

Direct paint mismatches are listed before colour-only residuals. Both remain FAIL under the fixed human-visibility policy; this grouping makes raw edge-pixel volume a secondary signal rather than the work order.

| direct evidence | fixtures | how to read it |
|-----------------|---------:|----------------|
| direct paint mismatch | 6 | Missing/Extra paint is the policy-triggering defect; inspect first |
| colour-only residual | 9 | colour/coverage is the policy-triggering defect; review at authored scale |

## Failure groups

Raster-output symptoms, not inferred root causes.

| raster symptom | fixtures |
|----------------|---------:|
| ColorValue | 8 |
| Missing | 5 |
| AntialiasCoverage | 1 |
| Extra | 1 |

## Needs attention

Integrity problems first, then all 15 rendering failure(s) and 10 disputed reference(s). A disputed reference retains its raw comparison evidence but is not a candidate verdict. The gate result is summarized once above. Support labels provide context only and never hide a defect. Generated-local visual inventory: `reports/index.html`.

| issue | category | fixture | detail |
|-------|----------|---------|--------|
| INTEGRITY | — | — | baseline.json is missing, invalid, or incompatible; regression comparison is unavailable |
| FAIL | interactions | [`interactions-cartesian-filters-x-overflow-clipping`](cases/interactions/interactions-cartesian-filters-x-overflow-clipping.html) | supported-family-cartesian-product · direct paint mismatch · Extra · max-page pixel diff 2.12% · 21566 differing RGBA pixels · candidate adds paint absent from reference (0.1%) |
| FAIL | tables | [`tables-fixed-unspecified-columns-ignore-font-size`](cases/tables/tables-fixed-unspecified-columns-ignore-font-size.html) | table-layout · direct paint mismatch · Missing · max-page pixel diff 1.10% · 2800 differing RGBA pixels · candidate lacks paint present in reference (1.2%) |
| FAIL | interactions | [`interactions-cartesian-transforms-x-units-values`](cases/interactions/interactions-cartesian-transforms-x-units-values.html) | supported-family-cartesian-product · direct paint mismatch · Missing · max-page pixel diff 0.83% · 8442 differing RGBA pixels · candidate lacks paint present in reference (0.2%) |
| FAIL | paged-media | [`table-row-taller-than-page`](cases/paged-media/table-row-taller-than-page.html) | fragmentation · direct paint mismatch · Missing · max-page pixel diff 0.61% · 3825 differing RGBA pixels · candidate lacks paint present in reference (1.1%) |
| FAIL | interactions | [`interactions-cartesian-positioning-x-text-advanced`](cases/interactions/interactions-cartesian-positioning-x-text-advanced.html) | supported-family-cartesian-product · direct paint mismatch · Missing · max-page pixel diff 0.23% · 2386 differing RGBA pixels · candidate lacks paint present in reference (0.1%) |
| FAIL | interactions | [`interactions-cartesian-text-advanced-x-typography`](cases/interactions/interactions-cartesian-text-advanced-x-typography.html) | supported-family-cartesian-product · direct paint mismatch · Missing · max-page pixel diff 0.13% · 1298 differing RGBA pixels · candidate lacks paint present in reference (0.04%) |
| FAIL | interactions | [`interactions-cartesian-filters-x-multicol`](cases/interactions/interactions-cartesian-filters-x-multicol.html) | supported-family-cartesian-product · colour-only residual · ColorValue · max-page pixel diff 2.94% · 29882 differing RGBA pixels · fill recolour ΔRGB(-1,-1,+0) (ΔE 39.8) |
| FAIL | interactions | [`interactions-cartesian-effects-x-paged-media`](cases/interactions/interactions-cartesian-effects-x-paged-media.html) | supported-family-cartesian-product · colour-only residual · ColorValue · max-page pixel diff 1.97% · 7402 differing RGBA pixels · page 2: fill recolour ΔRGB(-8,+13,+19) (ΔE 33.3) |
| FAIL | interactions | [`interactions-cartesian-paged-media-x-positioning`](cases/interactions/interactions-cartesian-paged-media-x-positioning.html) | supported-family-cartesian-product · colour-only residual · AntialiasCoverage · max-page pixel diff 1.58% · 5914 differing RGBA pixels · page 3: antialiasing coverage residue on a shared outline |
| FAIL | interactions | [`interactions-cartesian-multicol-x-paged-media`](cases/interactions/interactions-cartesian-multicol-x-paged-media.html) | supported-family-cartesian-product · colour-only residual · ColorValue · max-page pixel diff 1.28% · 742 differing RGBA pixels · page 4: fill recolour ΔRGB(-144,-128,-111) (ΔE 37.0) |
| FAIL | interactions | [`interactions-cartesian-generated-content-x-grid`](cases/interactions/interactions-cartesian-generated-content-x-grid.html) | supported-family-cartesian-product · colour-only residual · ColorValue · max-page pixel diff 0.63% · 6380 differing RGBA pixels · fill recolour ΔRGB(+12,+15,+17) (ΔE 64.8) |
| FAIL | interactions | [`interactions-cartesian-lists-counters-x-typography`](cases/interactions/interactions-cartesian-lists-counters-x-typography.html) | supported-family-cartesian-product · colour-only residual · ColorValue · max-page pixel diff 0.56% · 5678 differing RGBA pixels · fill recolour ΔRGB(+1,+4,+3) (ΔE 69.2) |
| FAIL | interactions | [`interactions-cartesian-grid-x-overflow-clipping`](cases/interactions/interactions-cartesian-grid-x-overflow-clipping.html) | supported-family-cartesian-product · colour-only residual · ColorValue · max-page pixel diff 0.48% · 4842 differing RGBA pixels · fill recolour ΔRGB(-3,-1,+1) (ΔE 69.2) |
| FAIL | interactions | [`interactions-cartesian-images-replaced-x-typography`](cases/interactions/interactions-cartesian-images-replaced-x-typography.html) | supported-family-cartesian-product · colour-only residual · ColorValue · max-page pixel diff 0.43% · 4330 differing RGBA pixels · fill recolour ΔRGB(+11,+67,+30) (ΔE 48.2) |
| FAIL | interactions | [`interactions-cartesian-images-replaced-x-text-advanced`](cases/interactions/interactions-cartesian-images-replaced-x-text-advanced.html) | supported-family-cartesian-product · colour-only residual · ColorValue · max-page pixel diff 0.26% · 2680 differing RGBA pixels · fill recolour ΔRGB(+5,+2,+0) (ΔE 8.3) |
| REFERENCE-DISPUTED | filters | [`r2-filter-url-feturbulence-displacement`](cases/filters/r2-filter-url-feturbulence-displacement.html) | filter: url(#id) with feTurbulence and feDisplacementMap · colour-only residual · ColorValue · max-page pixel diff 3.08% · 9688 differing RGBA pixels · REFERENCE DISPUTED: Filter Effects 1 section 9.21 supplies the exact feTurbulence algorithm: zero and over-length random gradient vectors are rejected and consume a new pseudorandom pair. Chromium PDF and Firefox screen output instead agree with the legacy normalize-every-pair field; authored position controls confirm Chromium still uses the required local coordinate system. Its PDF is therefore a compatibility canary, not a normative pixel oracle. · fill recolour ΔRGB(-1,-42,-42) (ΔE 45.7) |
| REFERENCE-DISPUTED | paged-media | [`paged-footnote-max-height`](cases/paged-media/paged-footnote-max-height.html) | footnote · direct paint mismatch · Missing · max-page pixel diff 2.06% · 6027 differing RGBA pixels · REFERENCE DISPUTED: CSS GCPM limits the footnote area with max-height except on a page containing only footnotes, and footnote-policy:auto permits the body to move to a later page. Both PDFs keep the call on page 1, move the complete body to the footnote-only page 2, and preserve the same three-line wrapping and rule geometry. Every body line in the WeasyPrint PDF is uniformly 0.1157pt lower; horizontal word bounds differ by at most 0.028pt. That renderer-specific baseline and glyph quantization is not prescribed by GCPM, so the WeasyPrint PDF is not a unique pixel oracle; raw shared-pdftoppm evidence remains reported. · page 2: candidate lacks paint present in reference (5.1%) |
| REFERENCE-DISPUTED | inline-text | [`inline-text-text-decoration-wavy`](cases/inline-text/inline-text-text-decoration-wavy.html) | text-decoration-style:wavy · direct paint mismatch · Missing · max-page pixel diff 2.05% · 5332 differing RGBA pixels · REFERENCE DISPUTED: CSS Text Decoration 4 section 2.2 requires only a wavy line, not Chrome's wavelength, amplitude, or jagged curve; Ironpress and WeasyPrint render different smooth waves. · candidate lacks paint present in reference (21.9%) |
| REFERENCE-DISPUTED | paged-media | [`footnote-float`](cases/paged-media/footnote-float.html) | footnote · direct paint mismatch · Missing · max-page pixel diff 1.88% · 22018 differing RGBA pixels · REFERENCE DISPUTED: CSS GCPM 3 section 2.6 defines the default footnote call as a superscripted counter with vertical-align:baseline, font-size:100%, line-height:inherit, and font-variant-position:super. Ironpress emits that superscript without moving the body line; WeasyPrint leaves the call full-size on the baseline, so its PDF is a compatibility canary, not a normative pixel oracle. · candidate lacks paint present in reference (22.8%) |
| REFERENCE-DISPUTED | generated-content | [`generated-content-string-set-running-header`](cases/generated-content/generated-content-string-set-running-header.html) | string-set and string() · direct paint mismatch · Missing · max-page pixel diff 1.29% · 5134 differing RGBA pixels · REFERENCE DISPUTED: CSS GCPM requires string-set to capture the heading and string() to reproduce it in the page-margin box; it does not prescribe renderer-specific glyph quantization. Both PDFs contain the complete Running Head on both pages. On page 1 Ironpress places its header bbox at x=47.8125..132.1758pt, y=4.2683..18.2243pt, versus WeasyPrint x=47.8184..132.2024pt, y=4.2683..18.2363pt. The residual is a text-contour representation difference, so this WeasyPrint PDF is not a unique pixel oracle; raw shared-pdftoppm evidence remains reported. · candidate lacks paint present in reference (2.3%) |
| REFERENCE-DISPUTED | tables | [`tables-colspan-max-clamp`](cases/tables/tables-colspan-max-clamp.html) | html-table-attributes · direct paint mismatch · Extra · max-page pixel diff 0.94% · 3096 differing RGBA pixels · REFERENCE DISPUTED: HTML clamps the first cell to 1000 columns, then forms a 1001st track for the following cell. CSS Tables fixed layout preserves every column and distributes unspecified-track width equally; Ironpress keeps that subpixel trailing cell, while Chromium gives it an 8px minimum and Firefox does not. The Chromium PDF is a compatibility canary, not a normative pixel oracle. · candidate adds paint absent from reference (0.2%) |
| REFERENCE-DISPUTED | overflow-clipping | [`overflow-scroll-print-clip`](cases/overflow-clipping/overflow-scroll-print-clip.html) | overflow · direct paint mismatch · Missing · max-page pixel diff 0.47% · 2555 differing RGBA pixels · REFERENCE DISPUTED: CSS Overflow 3 section 3.1.3 allows static-media UAs to show an overflow indication and says overflowing scroll content may be printed without defining where; Chrome's exact scrollbar and clipping pixels are not a normative oracle. · candidate lacks paint present in reference (0.6%) |
| REFERENCE-DISPUTED | overflow-clipping | [`overflow-axis-visible-hidden-coercion`](cases/overflow-clipping/overflow-axis-visible-hidden-coercion.html) | overflow · direct paint mismatch · Missing · max-page pixel diff 0.34% · 888 differing RGBA pixels · REFERENCE DISPUTED: CSS Overflow 3 section 3.1 makes visible compute to auto when the other axis is scrollable, which Ironpress implements. In print, auto inherits scroll's undefined overflow placement, so Chrome's scrollbar geometry and clipping pixels are a compatibility canary, not a normative oracle. · candidate lacks paint present in reference (0.3%) |
| REFERENCE-DISPUTED | lists-counters | [`lists-counters-marker-side-match-parent`](cases/lists-counters/lists-counters-marker-side-match-parent.html) | marker-side:match-parent · direct paint mismatch · Extra · max-page pixel diff 0.28% · 1449 differing RGBA pixels · expected partial · REFERENCE DISPUTED: CSS Lists 3 leaves the exact position of outside marker boxes undefined. This fixture does not vary list-item directionality, so match-parent has no observable effect. Chromium 150 reports CSS.supports('marker-side: match-parent') as false and an empty computed value; its PDF is an unsupported-feature compatibility canary, not a normative oracle. · candidate adds paint absent from reference (0.3%) |
| REFERENCE-DISPUTED | overflow-clipping | [`overflow-x-y-separate`](cases/overflow-clipping/overflow-x-y-separate.html) | overflow · direct paint mismatch · Extra · max-page pixel diff 0.23% · 1203 differing RGBA pixels · REFERENCE DISPUTED: CSS Overflow 3 section 3.1 makes visible compute to auto when the other axis is scrollable, which Ironpress implements. In static media, section 3.1.3 permits a UA overflow indication and section 5.1 leaves its appearance, size, and edge UA-defined; Chrome's scrollbar pixels are a compatibility canary, not a normative oracle. · candidate adds paint absent from reference (0.3%) |

## Categories — worst first

| category | verified visual parity | pass | fail | disputed refs |
|----------|-----------------------:|-----:|-----:|--------------:|
| [interactions](cases/interactions/) | 96.05% | 316 | 13 | 0 |
| [paged-media](cases/paged-media/) | 98.78% | 81 | 1 | 2 |
| [tables](cases/tables/) | 98.98% | 97 | 1 | 1 |
| [filters](cases/filters/) | 100.00% | 46 | 0 | 1 |
| [inline-text](cases/inline-text/) | 100.00% | 49 | 0 | 1 |
| [generated-content](cases/generated-content/) | 100.00% | 36 | 0 | 1 |
| [overflow-clipping](cases/overflow-clipping/) | 100.00% | 17 | 0 | 3 |
| [lists-counters](cases/lists-counters/) | 100.00% | 34 | 0 | 1 |
| [backgrounds-borders](cases/backgrounds-borders/) | 100.00% | 83 | 0 | 0 |
| [backgrounds-gradients](cases/backgrounds-gradients/) | 100.00% | 46 | 0 | 0 |
| [block-box-model](cases/block-box-model/) | 100.00% | 59 | 0 | 0 |
| [clip-mask](cases/clip-mask/) | 100.00% | 52 | 0 | 0 |
| [color-opacity](cases/color-opacity/) | 100.00% | 50 | 0 | 0 |
| [effects](cases/effects/) | 100.00% | 60 | 0 | 0 |
| [flexbox](cases/flexbox/) | 100.00% | 131 | 0 | 0 |
| [fonts-advanced](cases/fonts-advanced/) | 100.00% | 26 | 0 | 0 |
| [grid](cases/grid/) | 100.00% | 90 | 0 | 0 |
| [images-replaced](cases/images-replaced/) | 100.00% | 38 | 0 | 0 |
| [multicol](cases/multicol/) | 100.00% | 36 | 0 | 0 |
| [positioning](cases/positioning/) | 100.00% | 22 | 0 | 0 |
| [probes](cases/probes/) | 100.00% | 6 | 0 | 0 |
| [selectors-cascade](cases/selectors-cascade/) | 100.00% | 60 | 0 | 0 |
| [text-advanced](cases/text-advanced/) | 100.00% | 49 | 0 | 0 |
| [transforms](cases/transforms/) | 100.00% | 44 | 0 | 0 |
| [typography](cases/typography/) | 100.00% | 24 | 0 | 0 |
| [units-values](cases/units-values/) | 100.00% | 35 | 0 | 0 |

## Support labels

These labels describe intended surface coverage only. They never change a verdict; every non-PASS fixture remains in the needs-attention worklist above.

| expected support | total | pass | fail | disputed refs |
|------------------|------:|-----:|-----:|--------------:|
| partial | 37 | 36 | 0 | 1 |

## Run details

- Comparator: raw evidence is a shared upper-left canvas with white padding, no translation, registration, crop, filter, resampling, or replacement. The fixed visibility policy is applied directly to those pixels: paper ΔE2000 ≤2.3; a ColorErr pixel with every RGB channel delta ≤0.5% is semantically correct (its raw RGBA evidence remains reported); color ΔE2000 ≤2.3; edge color above that per-pixel allowance ≤1.50% of paint; interior color ≤0.125%; Missing/Extra component ≥4 CSS px²; unpaired component ≥8 CSS px span; disconnected total ≥16 CSS px². Balanced colour coverage requires page bias ≤0.10, every independently visible component (≥16 CSS px² or ≥16 CSS px span) bias ≤0.25, and direct unchanged anchors within one CSS px. A colour-ramp component may leave a corner/stem remainder only when at least 75% of its pixels directly prove the shared ramp and the remainder is below 16 CSS px²; a component wholly below that area floor still needs direct ramp evidence, no interior recolour, and one ink family. A mixed coverage phase additionally requires paired Missing/Extra ≤6.0% each, balance bias ≤0.05, ColorErr coverage ≥2× direct presence, component bounds below the normal glyph limits, interior colour ≤0.25%, an oriented shared paper/content ramp around every direct colour component, and either balanced colour energy or a hue-preserving ramp. A one-sided contour additionally requires ≥95% byte-identical shared paint, ≤1.0% direct presence, ColorErr ≥2× presence, and contour ΔE ≤7.0. A raw unpaired contour may pass only when every authored-space normal remains below one CSS pixel between directly shared paper and content; its total length is irrelevant because physical thickness, not raster-pixel count, controls visibility. Fragmented paired shared-outline coverage remains bounded to ≤1.0% of paint; a coherent outline may exceed that only with at most 4 direct components per sign. One-CSS-pixel strips, absent thin rules, inner cuts, and repeated glyph displacement remain failures. Every raw difference stays visible in the report. 300 DPI · source `/usr/bin/pdftoppm` · executed snapshot `/tmp/ironpress-pdftoppm-798836-1784733480945484327-0/pdftoppm` · argv `[-r, 300, -png, <PDF>, <PREFIX>]` · pdftoppm version 24.08.0 · binary SHA-256 `b1f76a56605df368efd233e09faad3bd910e50c0d6556c616a7c0b0adebf6013`.
- Reference lock: present · stale refs 0 · ref-name mismatches 0.
- Regression baseline: MISSING/INVALID/INCOMPATIBLE.
- Generated by `cargo test --test feature_parity`.
