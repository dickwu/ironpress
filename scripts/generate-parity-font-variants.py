#!/usr/bin/env python3
"""Generate deterministic font assets used by standards-derived oracles."""

from __future__ import annotations

import argparse
import io
import math
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "tests" / "parity" / "fonts" / "ParitySans.ttf"
OBLIQUE_20 = ROOT / "tests" / "parity" / "fonts" / "MatrixSansOblique20.woff2"
REFERENCE_CHARACTERS = "AgBbAB‹›0123456789."


def oblique_font() -> bytes:
    try:
        from fontTools.pens.recordingPen import DecomposingRecordingPen
        from fontTools.pens.transformPen import TransformPen
        from fontTools.pens.ttGlyphPen import TTGlyphPen
        from fontTools.subset import Options, Subsetter
        from fontTools.ttLib import TTFont
    except ImportError as error:
        raise SystemExit(
            "fontTools with WOFF2 support is required to generate parity font variants"
        ) from error

    font = TTFont(SOURCE, recalcBBoxes=True, recalcTimestamp=False)
    options = Options()
    options.hinting = False
    options.recalc_bounds = True
    options.recalc_timestamp = False
    options.name_IDs = [0, 1, 2, 3, 4, 5, 6]
    options.name_legacy = True
    options.name_languages = [0x409]
    options.drop_tables.append("FFTM")
    subsetter = Subsetter(options=options)
    subsetter.populate(text=REFERENCE_CHARACTERS)
    subsetter.subset(font)

    glyph_set = font.getGlyphSet()
    glyph_table = font["glyf"]
    shear = math.tan(math.radians(20))
    transformed = {}
    for glyph_name in font.getGlyphOrder():
        outline = DecomposingRecordingPen(glyph_set)
        glyph_set[glyph_name].draw(outline)
        destination = TTGlyphPen(None)
        outline.replay(TransformPen(destination, (1, 0, shear, 1, 0, 0)))
        glyph = destination.glyph()
        glyph.recalcBounds(glyph_table)
        transformed[glyph_name] = glyph

    glyph_table.glyphs.update(transformed)
    for glyph_name in font.getGlyphOrder():
        advance, _ = font["hmtx"][glyph_name]
        glyph = glyph_table[glyph_name]
        glyph.recalcBounds(glyph_table)
        font["hmtx"][glyph_name] = (advance, getattr(glyph, "xMin", 0))

    names = {
        1: "MatrixSans Oblique 20 Reference",
        2: "Regular",
        3: "MatrixSans Oblique 20 Reference 1.0",
        4: "MatrixSans Oblique 20 Reference",
        6: "MatrixSans-Oblique20-Reference",
    }
    for record in font["name"].names:
        if replacement := names.get(record.nameID):
            record.string = replacement.encode(record.getEncoding())
    font["post"].italicAngle = 0
    font["head"].macStyle &= ~0b10
    font["OS/2"].fsSelection &= ~0b1
    font.flavor = "woff2"

    output = io.BytesIO()
    font.save(output, reorderTables=False)
    return output.getvalue()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    expected = oblique_font()
    if args.check:
        if not OBLIQUE_20.is_file() or OBLIQUE_20.read_bytes() != expected:
            print(f"stale parity font variant: {OBLIQUE_20.relative_to(ROOT)}")
            return 1
        print("parity font variants are current")
        return 0
    OBLIQUE_20.write_bytes(expected)
    print(f"generated {OBLIQUE_20.relative_to(ROOT)} ({len(expected)} bytes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
