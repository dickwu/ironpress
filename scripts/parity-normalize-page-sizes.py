#!/usr/bin/env python3
"""Keep Chrome parity pages exactly representable at the 300 DPI test grid.

Chrome's PDF producer quantizes custom page dimensions.  At 300 DPI one CSS
pixel is 25/8 device pixels, so an integer CSS-pixel page dimension is exactly
representable only when it is a multiple of eight.  The parity comparator does
not crop or forgive a MediaBox mismatch; this tool prevents the fixture itself
from asking the Chrome oracle for a dimension it cannot emit exactly.

Only Chrome-oracle fixtures are considered.  Named/physical page sizes and
WeasyPrint fixtures are left alone and must be validated by their own oracle.
"""

from __future__ import annotations

import argparse
import json
import math
import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PARITY = ROOT / "tests" / "parity"
PAGE_SIZE = re.compile(
    r"(@page(?:\s+[^{}]+)?\s*\{[^{}]*?\bsize\s*:\s*)"
    r"(?P<width>\d+(?:\.\d+)?)px\s+(?P<height>\d+(?:\.\d+)?)px",
    re.IGNORECASE | re.DOTALL,
)


def chrome_fixture_paths() -> list[Path]:
    paths: set[Path] = set()
    for manifest_path in sorted((PARITY / "manifest").glob("*.json")):
        entries = json.loads(manifest_path.read_text(encoding="utf-8"))
        for entry in entries:
            if entry.get("oracle", "chrome") == "chrome":
                paths.add(PARITY / entry["file"])
    return sorted(paths)


def normalized(value: str) -> int:
    return int(math.ceil(float(value) / 8.0) * 8)


def rewrite(source: str) -> tuple[str, int]:
    changed = 0

    def replace(match: re.Match[str]) -> str:
        nonlocal changed
        width = normalized(match.group("width"))
        height = normalized(match.group("height"))
        replacement = f"{match.group(1)}{width}px {height}px"
        if replacement != match.group(0):
            changed += 1
        return replacement

    return PAGE_SIZE.sub(replace, source), changed


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--write",
        action="store_true",
        help="rewrite non-representable Chrome fixture page sizes",
    )
    args = parser.parse_args()

    changed_files: list[tuple[Path, int]] = []
    for path in chrome_fixture_paths():
        source = path.read_text(encoding="utf-8")
        output, count = rewrite(source)
        if count == 0:
            continue
        changed_files.append((path, count))
        if args.write:
            path.write_text(output, encoding="utf-8")

    total = sum(count for _, count in changed_files)
    if args.write:
        print(f"normalized {total} @page size declaration(s) in {len(changed_files)} fixture(s)")
        return 0
    if changed_files:
        print(
            f"{total} Chrome @page size declaration(s) are not exactly representable at 300 DPI; "
            "run scripts/parity-normalize-page-sizes.py --write",
            file=sys.stderr,
        )
        for path, count in changed_files[:20]:
            print(f"  {path.relative_to(ROOT)} ({count})", file=sys.stderr)
        if len(changed_files) > 20:
            print(f"  ... and {len(changed_files) - 20} more", file=sys.stderr)
        return 1
    print("Chrome parity page sizes are exactly representable at 300 DPI")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
