#!/usr/bin/env python3
"""Generate the complete supported feature-family interaction product.

Each unordered pair, including the diagonal, gets its own fixture.  A fixture
contains three compositions: both families on one element, A outside B, and B
outside A.  The source manifest is derived from the non-generated feature
manifests, so adding a supported family makes this generator fail until a
representative is defined. Cross-cutting page context is included explicitly
because it cannot be represented by an element-level feature category.

Usage:
    scripts/generate-interaction-cartesian.py
    scripts/generate-interaction-cartesian.py --check
"""

from __future__ import annotations

import argparse
import base64
import itertools
import json
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PARITY = ROOT / "tests" / "parity"
MANIFESTS = PARITY / "manifest"
GENERATED_MANIFEST = MANIFESTS / "interactions.json"
CASES = PARITY / "cases" / "interactions"
REFERENCES = PARITY / "references" / "interactions"
PREFIX = "interactions-cartesian-"
CONTROL_ID = "interaction-product-carrier-control"
OBLIQUE_REFERENCE_FONT = PARITY / "fonts" / "MatrixSansOblique20.woff2"

STATIC_REFERENCE_PAIRS = {
    ("clip-mask", "multicol"),
    ("grid", "paged-media"),
    ("multicol", "paged-media"),
}

IMAGE_DATA = (
    "data:image/png;base64,"
    "iVBORw0KGgoAAAANSUhEUgAAAAgAAAAECAIAAAA8r+mnAAAAIElEQVR42mM4IScHR3I9NnDEgFNCzu0EHN1ZJQJHOCUAni4lgeO2HLIAAAAASUVORK5CYII="
)


@dataclass(frozen=True)
class Family:
    slug: str
    css: str
    forces_pagination: bool = False


@dataclass(frozen=True)
class CssRule:
    selector: str
    declaration: str


@dataclass(frozen=True)
class ReferenceMaterialization:
    rules: tuple[CssRule, ...]
    rationale: str
    suffix_css: str = ""


@dataclass(frozen=True)
class ReferenceAssessment:
    status: str
    note: str


FAMILIES = {
    family.slug: family
    for family in (
        Family(
            "backgrounds-borders",
            "border:3px double #355070;border-radius:12px;background-color:#dceeff;",
        ),
        Family(
            "backgrounds-gradients",
            "background-image:linear-gradient(135deg,rgba(255,209,102,.82),rgba(6,214,160,.42));",
        ),
        Family(
            "block-box-model",
            "box-sizing:border-box;padding:11px 7px;min-width:92px;max-width:132px;",
        ),
        Family("clip-mask", "clip-path:polygon(5% 0,100% 8%,94% 100%,0 88%);"),
        Family("color-opacity", "color:#5a189a;opacity:.76;"),
        Family("effects", "box-shadow:7px 5px 0 rgba(239,71,111,.35),inset 0 0 0 2px #ffd166;"),
        Family("filters", "filter:grayscale(.18) contrast(1.08) drop-shadow(2px 1px 0 #90a4ae);"),
        Family("flexbox", "display:flex;align-items:center;justify-content:space-around;gap:4px;"),
        Family("fonts-advanced", "font-family:MatrixSans;font-style:oblique 20deg;"),
        Family("generated-content", ""),
        Family("grid", "display:grid;grid-template-columns:1fr 1fr;align-items:center;gap:3px;"),
        Family("images-replaced", ""),
        Family("inline-text", "line-height:1.35;word-spacing:5px;"),
        Family("lists-counters", "counter-reset:pair-item;"),
        Family("multicol", "column-count:2;column-gap:7px;column-rule:2px solid #577590;"),
        Family("overflow-clipping", "overflow:hidden;max-height:58px;border-radius:9px;"),
        Family(
            "page-margins",
            "",
            forces_pagination=True,
        ),
        Family(
            "paged-media",
            "break-before:page;break-inside:avoid;",
            forces_pagination=True,
        ),
        Family("positioning", "position:relative;"),
        Family("selectors-cascade", ""),
        Family("tables", "display:table;border-collapse:separate;border-spacing:3px;"),
        Family(
            "text-advanced",
            "text-decoration-line:underline;text-decoration-style:solid;"
            "text-decoration-color:#ef476f;text-decoration-thickness:2px;"
            "text-underline-offset:3px;text-shadow:1px 1px 0 #fff;",
        ),
        Family("transforms", "transform:translate(2px,-1px) rotate(5deg);transform-origin:20% 70%;"),
        Family("typography", "font-size:18px;font-weight:700;letter-spacing:.7px;"),
        Family("units-values", "width:calc(100% - 9px);padding-left:5%;"),
    )
}

CROSS_CUTTING_FAMILIES = {"page-margins"}


ATOMIC_MULTICOL_REFERENCES = {
    ("flexbox", "multicol"): ReferenceMaterialization(
        rules=(
            CssRule(
                "body > .stage:nth-child(3) > .f-multicol > .inner",
                "break-before: column;",
            ),
        ),
        rationale="the unbreakable flex child establishes the balanced column height",
    ),
    ("grid", "multicol"): ReferenceMaterialization(
        rules=(
            CssRule(
                "body > .stage:nth-child(1) > .f-grid",
                "column-rule-style: none;",
            ),
        ),
        rationale="the unbreakable grid child establishes the balanced column height",
        suffix_css="""  /* The second stage has one line in one actual column. CSS Multicol 1
     section 2 allows the actual count to be lower than the used count,
     and section 4 paints rules only between columns that both have content. */
  body > .stage:nth-child(2) .f-multicol {column-rule-style: none;}
  body > .stage:nth-child(3) > .f-multicol > .inner {break-before: column;}
  /* CSS Multicol 1 sections 2 and 3.4 require equal-width columns:
     (108px + 7px) / 2 - 7px = 50.5px. Chromium's live fragment starts
     at x=425.5px, but print-to-PDF snaps only that second fragment to
     x=426px. Undo the exporter-only half-pixel shift after painting. */
  body > .stage:nth-child(3) > .f-multicol > .inner {
    position: relative;
    z-index: 0;
    background: transparent;
    border-color: transparent;
  }
  body > .stage:nth-child(3) > .f-multicol > .inner::before {
    content: "";
    position: absolute;
    z-index: -1;
    left: -2px;
    top: -2px;
    width: 58px;
    height: 48px;
    box-sizing: border-box;
    border: 2px solid #577590;
    background: #e7f5ff;
    transform: translateX(-.5px);
  }
""",
    ),
    ("multicol", "tables"): ReferenceMaterialization(
        rules=(
            CssRule(
                "body > .stage:nth-child(2) > .f-multicol > .inner",
                "break-before: column;",
            ),
        ),
        rationale="the unbreakable table child establishes the balanced column height",
    ),
}


PAIR_DESCRIPTION_DETAILS = {
    (
        "clip-mask",
        "multicol",
    ): "The standards-derived reference corrects Chromium's print-only half-pixel clip-path shift on the second fractional-width column.",
    (
        "grid",
        "multicol",
    ): "The standards-derived reference suppresses Chromium's rule beside an empty actual column, forces the CSS Multicol boundary Chromium misses, and corrects its print-only half-pixel second-fragment decoration snap while retaining Chromium's live text geometry.",
    (
        "transforms",
        "units-values",
    ): "Percentage sizing and transform origins resolve against each box before cumulative transform matrices map the painted descendants.",
}


PAIR_REFERENCE_ASSESSMENTS = {
    ("effects", "paged-media"): ReferenceAssessment(
        status="disputed",
        note=(
            "CSS Fragmentation 3 section 5.4 requires box-decoration-break:slice "
            "to render the unbroken box and then slice it, with no box-shadow "
            "drawn at a broken edge. WPT box-shadow-002 tests that rule directly, "
            "while box-shadow-005 reserves a shadow around every fragment for "
            "box-decoration-break:clone. Ironpress leaves the page 2/3 cut "
            "undecorated; Chromium paints the yellow inset-shadow band at both "
            "cut edges as if the fragments were cloned. Its PDF remains a "
            "compatibility canary, not a normative oracle, and the raw shared-"
            "pdftoppm evidence remains reported."
        ),
    ),
    ("positioning", "tables"): ReferenceAssessment(
        status="disputed",
        note=(
            "CSS Tables 3 section 4.1 defines the containing block generated by "
            "a positioned table wrapper as the area around which table margins "
            "are applied, explicitly including the area where the table border "
            "is drawn. Ironpress positions the absolute Bb from that border edge. "
            "Both the locked Chromium 150.0.7871.114 Foundation PDF and a fresh "
            "Chromium 150.0.7871.128 Foundation PDF instead inset it by the "
            "authored 2px table border on both axes; the specification identifies "
            "this behavior as a Chromium bug and interoperability risk. The "
            "Chromium PDF remains a compatibility canary rather than a normative "
            "geometry oracle, and the raw shared-pdftoppm evidence remains reported."
        ),
    ),
    ("page-margins", "paged-media"): ReferenceAssessment(
        status="disputed",
        note=(
            "CSS Fragmentation 3 sections 3.1 and 4.3 require break-before:page "
            "on the nested in-flow block to force a class-A page break and move "
            "the ensuing content into the next page fragmentainer. Ironpress "
            "therefore emits four pages and slices the outer box across pages "
            "2 and 3. Chromium 150 Foundation ignores that forced break, emits "
            "three pages, and overlaps the nested text with its preceding "
            "sibling on page 2. Its PDF remains a compatibility canary rather "
            "than a normative fragmentation oracle; the complete shared-"
            "pdftoppm page-count and raster evidence remains reported."
        ),
    ),
}


SHARED_CSS_TEMPLATE = """
  @font-face {
    font-family: MatrixSans;
    src: url('../../fonts/ParitySans.ttf') format('truetype');
    font-style: normal;
    font-weight: 400;
  }
  html { font-family: ParitySans; font-size: 16px; line-height: 1.2; }
  * { box-sizing: border-box; margin: 0; }
  html, body { background: #ffffff; }
  body { padding: 16px; color: #17324d; font-size: 0; white-space: nowrap; }
  .stage {
    display: inline-flex;
    width: 156px;
    __STAGE_BLOCK_CONSTRAINT__
    margin-right: 8px;
    align-items: center;
    justify-content: center;
    background: #f7fafc;
    border: 1px solid #cbd5e1;
    font-size: 16px;
    vertical-align: top;
  }
  .stage:last-child { margin-right: 0; }
  .node {
    width: 126px;
    __NODE_BLOCK_CONSTRAINT__
    padding: 7px;
    border: 2px solid #577590;
    background-color: #e7f5ff;
    color: #17324d;
  }
  .node.inner {
    width: 58px;
    height: 48px;
    padding: 5px;
  }
  .node.outer { __OUTER_BLOCK_CONSTRAINT__ }
  .own { height: 22px; white-space: nowrap; }
  .asset { display: none; width: 34px; height: 24px; object-fit: cover; object-position: 70% 50%; }
  .f-generated-content::before { content: '‹'; color: #d62828; font-weight: 700; }
  .f-generated-content::after { content: '›'; color: #2a9d8f; font-weight: 700; }
  .f-images-replaced > .own > .asset { display: inline-block; }
  .f-inline-text > .own > .token:first-of-type { font-size: .72em; vertical-align: super; }
  .f-lists-counters > .own > .token { counter-increment: pair-item; }
  .f-lists-counters > .own > .token::before { content: counter(pair-item) '.'; color: #d62828; }
  .f-positioning > .own > .token:last-of-type { position: absolute; right: 4px; bottom: 4px; }
  .f-selectors-cascade:is(.node) > .own > .token:nth-of-type(2) { color: #c1121f; }
  @supports (display:grid) {
    .f-selectors-cascade { border-left-color: #06d6a0; border-right-color: #06d6a0; }
  }
  .f-tables > .own > .token { display: table-cell; vertical-align: middle; }
"""


def shared_css(paged: bool) -> str:
    constraints = {
        "    __STAGE_BLOCK_CONSTRAINT__\n": (
            "    min-height: 164px;\n" if paged else "    height: 164px;\n"
        ),
        "    __NODE_BLOCK_CONSTRAINT__\n": (
            "" if paged else "    height: 68px;\n"
        ),
        "  .node.outer { __OUTER_BLOCK_CONSTRAINT__ }\n": (
            "  .node.outer { min-height: 96px; }\n"
            if paged
            else "  .node.outer { height: 96px; }\n"
        ),
    }
    css = SHARED_CSS_TEMPLATE
    for placeholder, constraint in constraints.items():
        css = css.replace(placeholder, constraint)
    return css


def source_families() -> set[str]:
    families: set[str] = set()
    for path in sorted(MANIFESTS.rglob("*.json")):
        if GENERATED_MANIFEST == path:
            continue
        for entry in json.loads(path.read_text(encoding="utf-8")):
            if (
                entry.get("kind", "feature") == "feature"
                and entry.get("expected_support", "implemented") != "unsupported"
                and entry["category"] not in {"interactions", "probes"}
            ):
                families.add(entry["category"])
    return families


def validate_registry(families: set[str]) -> None:
    registered = set(FAMILIES) - CROSS_CUTTING_FAMILIES
    if families != registered:
        missing = sorted(families - registered)
        stale = sorted(registered - families)
        raise SystemExit(
            "interaction family registry does not match supported manifests: "
            f"missing={missing}, stale={stale}"
        )


def class_rule(family: Family) -> str:
    return f"  .f-{family.slug} {{{family.css}}}\n"


def reference_class_rule(family: Family) -> str:
    if family.slug == "fonts-advanced":
        return (
            "  .f-fonts-advanced {"
            "font-family:MatrixSansOblique20Reference;font-style:normal;}\n"
        )
    return class_rule(family)


def node(classes: str, nested: str = "", role: str = "") -> str:
    token_a, token_b = ("A", "B") if role == "inner" else ("Ag", "Bb")
    return (
        f'<div class="node {role} {classes}">'
        f'<div class="own"><span class="token">{token_a}</span>'
        f'<span class="token">{token_b}</span>'
        f'<img class="asset" alt="" src="{IMAGE_DATA}"></div>{nested}</div>'
    )


def interaction_stages(first: Family, second: Family) -> list[str]:
    return [
        node(f"f-{first.slug} f-{second.slug}"),
        node(f"f-{first.slug}", node(f"f-{second.slug}", role="inner"), role="outer"),
        node(f"f-{second.slug}", node(f"f-{first.slug}", role="inner"), role="outer"),
    ]


def page_rule(families: tuple[Family, ...]) -> str:
    slugs = {family.slug for family in families}
    if "page-margins" in slugs:
        return """@page {
    size: 192px 200px;
    margin: 16px;
    background: #ffd6d6;
  }
  @page :left { margin: 8px 32px 24px 8px; }
  @page :right { margin: 16px 8px 8px 32px; }
  @page :first { margin: 32px 8px 16px 24px; }"""
    if any(family.forces_pagination for family in families):
        return "@page { size: 192px 200px; margin: 0; }"
    return "@page { size: 520px 200px; margin: 0; }"


def document(
    title: str,
    pair_css: str,
    stages: list[str],
    families: tuple[Family, ...] = (),
) -> str:
    # Integer CSS-pixel dimensions must be multiples of eight to map exactly at
    # the pinned 300 DPI (one CSS pixel is 3.125 raster pixels).
    document_page_rule = page_rule(families)
    paged = any(family.forces_pagination for family in families)
    has_page_margins = "page-margins" in {family.slug for family in families}
    # Keep paged interactions in ordinary block flow. A flex carrier would add
    # flex fragmentation to every nominally pairwise paged-media fixture, and
    # CSS Flexbox deliberately leaves that exact fragmented layout undefined.
    paged_css = """
  body { display: block; padding: 0; font-size: 16px; white-space: normal; }
  .stage { display: block; margin: 0; break-before: page; }
  .stage > .node { min-height: 68px; margin: 47px auto 0; }
  .stage > .node.outer { margin-top: 33px; }
""" if paged else ""
    page_margin_css = """
  /* The three compositions occupy first/right, left, and later-right pages.
     Their asymmetric page areas expose side selection, cascade, content
     origin, bottom capacity, and paint that crosses a page-area edge.
     Horizontal body padding is a distinct root-flow gutter: each stage
     deliberately shifts through it to the physical page-area start edge. */
  body { padding-left: 8px; padding-right: 8px; }
  .stage {
    width: 152px;
    height: 128px;
    min-height: 128px;
    margin-left: -8px;
    border: 0;
    background: #d8f3dc;
  }
  .node { width: 100%; }
  .stage > .node,
  .stage > .node.outer { margin: 0; }
""" if has_page_margins else ""
    return f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>{title}</title>
<style>
  {document_page_rule}
{shared_css(paged)}{paged_css}{page_margin_css}{pair_css}</style>
</head>
<body>
  <div class="stage">{stages[0]}</div>
  <div class="stage">{stages[1]}</div>
  <div class="stage">{stages[2]}</div>
</body>
</html>
"""


def fixture_html(first: Family, second: Family) -> str:
    pair_css = class_rule(first)
    if second != first:
        pair_css += class_rule(second)
    return document(
        f"{first.slug} x {second.slug} interaction",
        pair_css,
        interaction_stages(first, second),
        families=(first, second),
    )


def oblique_reference_html(first: Family, second: Family) -> str:
    encoded_font = base64.b64encode(OBLIQUE_REFERENCE_FONT.read_bytes()).decode("ascii")
    pair_css = (
        "  /* CSS Fonts 4 requires the authored 20deg synthetic oblique angle.\n"
        "     Chromium substitutes its 14deg default, so this oracle uses the\n"
        "     same bundled outlines pre-sheared 20deg about the baseline. */\n"
        "  @font-face {\n"
        "    font-family: MatrixSansOblique20Reference;\n"
        f"    src: url('data:font/woff2;base64,{encoded_font}') format('woff2');\n"
        "    font-style: normal;\n"
        "    font-weight: 400;\n"
        "  }\n"
    )
    pair_css += reference_class_rule(first)
    if second != first:
        pair_css += reference_class_rule(second)
    return document(
        f"{first.slug} x {second.slug} interaction — standards-derived reference",
        pair_css,
        interaction_stages(first, second),
        families=(first, second),
    )


def atomic_multicol_reference_html(first: Family, second: Family) -> str:
    materialization = ATOMIC_MULTICOL_REFERENCES[(first.slug, second.slug)]
    pair_css = class_rule(first)
    if second != first:
        pair_css += class_rule(second)
    pair_css += (
        "  /* CSS Multicol 1 section 7.1 establishes the shortest balanced\n"
        "     column height, then fills columns sequentially. Chromium offsets\n"
        f"     column two even though {materialization.rationale}. Force only\n"
        "     that required column boundary in this reference. CSS Multicol's\n"
        "     block-container applicability also suppresses grid-only rules. */\n"
    )
    pair_css += "".join(
        f"  {rule.selector} {{{rule.declaration}}}\n" for rule in materialization.rules
    )
    pair_css += materialization.suffix_css
    return document(
        f"{first.slug} x {second.slug} interaction — standards-derived reference",
        pair_css,
        interaction_stages(first, second),
        families=(first, second),
    )


def reference_file(first: Family, second: Family) -> str | None:
    pair = (first.slug, second.slug)
    if (
        "fonts-advanced" in pair
        or pair in STATIC_REFERENCE_PAIRS
        or pair in ATOMIC_MULTICOL_REFERENCES
    ):
        fixture_id = f"{PREFIX}{first.slug}-x-{second.slug}"
        return f"references/interactions/{fixture_id}.html"
    return None


def carrier_html() -> str:
    return document(
        "interaction product neutral carrier control",
        "",
        [
            node(""),
            node("", node("", role="inner"), role="outer"),
            node("", node("", role="inner"), role="outer"),
        ],
    )


def manifest_entry(first: Family, second: Family) -> dict[str, object]:
    fixture_id = f"{PREFIX}{first.slug}-x-{second.slug}"
    pair = (first.slug, second.slug)
    if "page-margins" in pair:
        other = second if first.slug == "page-margins" else first
        if other.slug == "page-margins":
            description = (
                "Page-margin self-interaction across first/right, left, and "
                "later-right physical pages. Intentionally asymmetric margins "
                "make page-side selection, cascade, content origins, available "
                "block size, page-area paint boundaries, and root-flow gutters "
                "conspicuous."
            )
        else:
            description = (
                f"Physical-page-context interaction for page-margins and {other.slug}: "
                f"the {other.slug} representative is exercised on the stage subject "
                "and at both nesting depths across first/right, left, and later-right "
                "physical pages. Intentionally asymmetric margins put layout and "
                "graphical effects directly against conspicuous page-area boundaries; "
                "stages also shift through root padding to detect conflated clips."
            )
        subfeature = "physical-page-context-and-nested-feature-depths"
    else:
        detail = PAIR_DESCRIPTION_DETAILS.get(
            pair,
            "Paged-media representatives force actual page breaks.",
        )
        description = (
            f"Cartesian family interaction for {first.slug} and {second.slug}: "
            "both representatives compose on one element and in both outer/inner orders. "
            f"{detail}"
        )
        subfeature = "same-element-and-bidirectional-nesting"
    entry: dict[str, object] = {
        "id": fixture_id,
        "category": "interactions",
        "feature": "supported-family-cartesian-product",
        "subfeature": subfeature,
        "description": description,
        "file": f"cases/interactions/{fixture_id}.html",
        "interaction_of": [first.slug, second.slug],
        "kind": "interaction",
        "oracle": "chrome",
        "expected_support": "implemented",
        "depends_on": [
            "probe-text-baseline",
            "probe-fill-box",
            "probe-border-box",
            "probe-color-swatch",
        ],
    }
    if path := reference_file(first, second):
        entry["reference_file"] = path
    if assessment := PAIR_REFERENCE_ASSESSMENTS.get(pair):
        entry["reference"] = {
            "status": assessment.status,
            "note": assessment.note,
        }
    return entry


def generated_files() -> dict[Path, str]:
    families = source_families()
    validate_registry(families)
    generated: dict[Path, str] = {}
    manifest: list[dict[str, object]] = []
    if GENERATED_MANIFEST.is_file():
        manifest.extend(
            entry
            for entry in json.loads(GENERATED_MANIFEST.read_text(encoding="utf-8"))
            if entry.get("id") != CONTROL_ID
            and not str(entry.get("id", "")).startswith(PREFIX)
        )
    ordered = [FAMILIES[slug] for slug in sorted(FAMILIES)]
    for first, second in itertools.combinations_with_replacement(ordered, 2):
        entry = manifest_entry(first, second)
        path = PARITY / str(entry["file"])
        generated[path] = fixture_html(first, second)
        if "fonts-advanced" in {first.slug, second.slug}:
            reference_path = PARITY / str(entry["reference_file"])
            generated[reference_path] = oblique_reference_html(first, second)
        elif (first.slug, second.slug) in ATOMIC_MULTICOL_REFERENCES:
            reference_path = PARITY / str(entry["reference_file"])
            generated[reference_path] = atomic_multicol_reference_html(first, second)
        manifest.append(entry)
    control_file = f"cases/interactions/{CONTROL_ID}.html"
    generated[PARITY / control_file] = carrier_html()
    manifest.append(
        {
            "id": CONTROL_ID,
            "category": "interactions",
            "feature": "interaction-product-carrier",
            "subfeature": "neutral-same-element-and-nested-control",
            "description": "Neutral control for the generated Cartesian carrier: one plain node and two identical nested-node compositions, with no family representative styles.",
            "file": control_file,
            "kind": "probe",
            "oracle": "chrome",
            "expected_support": "implemented",
            "depends_on": ["probe-text-baseline", "probe-fill-box", "probe-border-box"],
        }
    )
    generated[GENERATED_MANIFEST] = json.dumps(manifest, indent=2, ensure_ascii=False) + "\n"
    return generated


def stale_generated_cases(expected: set[Path]) -> list[Path]:
    candidates = list(CASES.glob(f"{PREFIX}*.html")) + [CASES / f"{CONTROL_ID}.html"]
    return sorted(path for path in candidates if path.is_file() and path not in expected)


def stale_generated_references(expected: set[Path]) -> list[Path]:
    candidates = {
        *REFERENCES.glob(f"{PREFIX}fonts-advanced-x-*.html"),
        *REFERENCES.glob(f"{PREFIX}*-x-fonts-advanced.html"),
        *(REFERENCES / f"{PREFIX}{first}-x-{second}.html"
          for first, second in ATOMIC_MULTICOL_REFERENCES),
    }
    return sorted(path for path in candidates if path.is_file() and path not in expected)


def generated_counts(files: dict[Path, str]) -> tuple[int, int]:
    pairs = sum(
        path.parent == CASES and path.name.startswith(PREFIX) for path in files
    )
    references = sum(path.parent == REFERENCES for path in files)
    return pairs, references


def check(files: dict[Path, str]) -> int:
    problems = []
    for path, expected in files.items():
        if not path.is_file():
            problems.append(f"missing {path.relative_to(ROOT)}")
        elif path.read_text(encoding="utf-8") != expected:
            problems.append(f"stale {path.relative_to(ROOT)}")
    problems.extend(
        f"unexpected {path.relative_to(ROOT)}" for path in stale_generated_cases(set(files))
    )
    problems.extend(
        f"unexpected {path.relative_to(ROOT)}"
        for path in stale_generated_references(set(files))
    )
    if problems:
        print("interaction Cartesian generation is stale:")
        print("\n".join(f"  - {problem}" for problem in problems))
        return 1
    pairs, references = generated_counts(files)
    print(
        "interaction Cartesian generation is current: "
        f"{pairs} pairs + {references} standards-derived references + carrier control"
    )
    return 0


def write(files: dict[Path, str]) -> None:
    for path in stale_generated_cases(set(files)):
        path.unlink()
    for path in stale_generated_references(set(files)):
        path.unlink()
    for path, content in files.items():
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")
    pairs, references = generated_counts(files)
    print(
        f"generated {pairs} Cartesian interaction pairs + "
        f"{references} standards-derived references + carrier control"
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    files = generated_files()
    if args.check:
        return check(files)
    write(files)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
