"""material-cdn glyph step: icons that upstream ships no static SVG for, drawn from the variable font.

    uv run --with fonttools tools/material-cdn/glyphs.py FONT.ttf OUT.json CP[,CP...]

The outline is read at opsz 24, GRAD 0 and every weight of the static tree (100..700, FILL 0 and 1) and written in the coordinate space of
the upstream SVGs (viewBox "0 -960 960 960", the font's em is 960 and y runs down there). The FILL glyph is the one the font's GSUB swaps the
icon to (the same data fonts.py writes as `fill`); an icon without such a swap keeps its outline.
OUT.json: {"<codepoint>": {"<wght>": {"0": "<path d>", "1": "<path d>"}}}.
"""

import json
import re
import sys

from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen
from fontTools.ttLib import TTFont

WEIGHTS = (100, 200, 300, 400, 500, 600, 700)


def compact(path: str) -> str:
    return re.sub(r" -", "-", re.sub(r" ?([A-Za-z]) ?", r"\1", path)).strip()


def main() -> None:
    ttf, out, wanted = sys.argv[1], sys.argv[2], [int(cp, 16) for cp in sys.argv[3].split(",")]
    font = TTFont(ttf)
    cmap = font.getBestCmap()
    swap: dict[str, str] = {}
    for lookup in font["GSUB"].table.LookupList.Lookup:
        for sub in lookup.SubTable:
            sub = getattr(sub, "ExtSubTable", sub)
            if hasattr(sub, "mapping"):
                swap.update(sub.mapping)
    if font["head"].unitsPerEm != 960:
        raise SystemExit(f"{ttf}: unitsPerEm {font['head'].unitsPerEm}, the upstream viewBox needs 960")
    result: dict[str, dict[str, dict[str, str]]] = {}
    for cp in wanted:
        base = cmap.get(cp)
        if base is None:
            raise SystemExit(f"{ttf}: no glyph for U+{cp:04X}")
        glyphs = {0: base, 1: swap.get(base, base)}
        result[str(cp)] = {}
        for weight in WEIGHTS:
            drawn: dict[str, str] = {}
            for fill, name in glyphs.items():
                glyph_set = font.getGlyphSet(location={"wght": weight, "FILL": fill, "GRAD": 0, "opsz": 24})
                pen = SVGPathPen(glyph_set, ntos=lambda value: f"{value:.2f}".rstrip("0").rstrip("."))
                glyph_set[name].draw(TransformPen(pen, (1, 0, 0, -1, 0, 0)))
                drawn[str(fill)] = compact(pen.getCommands())
            result[str(cp)][str(weight)] = drawn
    with open(out, "w") as fh:
        json.dump(result, fh, separators=(",", ":"))


if __name__ == "__main__":
    main()
