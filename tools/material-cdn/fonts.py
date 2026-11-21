"""material-cdn font step: variable TTF -> full WOFF2 plus unicode-range WOFF2 subsets.

    uv run --with fonttools --with brotli tools/material-cdn/fonts.py OUT_DIR SPEC.json

SPEC.json: [{"slug": "...", "ttf": "path", "pin": {"slnt": 0, "wdth": 100}, "ranges": {"latin": "U+0000-00FF,...", ...}}]
`pin` (optional) pins or limits variation axes first (fontTools.varLib.instancer); a number pins, a [min, max] pair limits.
`symbols` (optional, {"codepoints": path, "out": path}) marks a Material Symbols font: no WOFF2 files are written; instead `out` gets
{"names": [...], "cp": [...], "fill": [...]} (parallel arrays sorted by name; `fill` is the glyph id the FILL axis swaps the icon to, 0 when it has none)
read from the font's GSUB FeatureVariations, which the Worker needs to keep the filled glyphs of an icon subset.
Prints a JSON report on stdout: [{"slug", "ttf", "axes", "glyphs", "cmap" (merged codepoint ranges), "files": [{"subset", "file", "bytes", "range"}]}].
Subsets keep every layout feature and every variation axis; hinting is dropped (variable fonts render unhinted).
"""

import json
import os
import sys

from fontTools import subset
from fontTools.ttLib import TTFont
from fontTools.varLib import instancer


def parse_ranges(spec: str) -> set[int]:
    out: set[int] = set()
    for part in spec.split(","):
        part = part.strip().upper().removeprefix("U+")
        if "-" in part:
            lo, hi = part.split("-")
            out.update(range(int(lo, 16), int(hi, 16) + 1))
        else:
            out.add(int(part, 16))
    return out


def subset_font(ttf: str, out: str, unicodes: set[int]) -> None:
    opts = subset.Options()
    opts.flavor = "woff2"
    opts.layout_features = ["*"]
    opts.hinting = False
    opts.notdef_outline = True
    opts.name_IDs = ["*"]
    opts.drop_tables = list(set(opts.drop_tables) | {"DSIG"})
    font = subset.load_font(ttf, opts)
    sub = subset.Subsetter(opts)
    sub.populate(unicodes=unicodes)
    sub.subset(font)
    subset.save_font(font, out, opts)


def merge_ranges(codepoints: set[int]) -> list[list[int]]:
    ranges: list[list[int]] = []
    for cp in sorted(codepoints):
        if ranges and cp == ranges[-1][1] + 1:
            ranges[-1][1] = cp
        else:
            ranges.append([cp, cp])
    return ranges


def symbol_data(font: TTFont, codepoints: str, out: str) -> int:
    order = font.getGlyphOrder()
    gid = {name: index for index, name in enumerate(order)}
    swap: dict[str, str] = {}
    gsub = font["GSUB"].table
    for lookup in gsub.LookupList.Lookup:
        for sub in lookup.SubTable:
            sub = getattr(sub, "ExtSubTable", sub)
            if hasattr(sub, "mapping"):
                swap.update(sub.mapping)
    cmap = font.getBestCmap()
    rows = []
    with open(codepoints) as fh:
        for line in fh:
            parts = line.split()
            if len(parts) == 2:
                rows.append((parts[0], int(parts[1], 16)))
    rows.sort()
    names, cps, fills = [], [], []
    for name, cp in rows:
        base = cmap.get(cp)
        if base is None:
            continue
        names.append(name)
        cps.append(cp)
        fills.append(gid[swap[base]] if base in swap else 0)
    with open(out, "w") as fh:
        json.dump({"names": names, "cp": cps, "fill": fills}, fh, separators=(",", ":"))
    return len(names)


def main() -> None:
    out_dir, spec_path = sys.argv[1], sys.argv[2]
    report = []
    with open(spec_path) as fh:
        specs = json.load(fh)
    for spec in specs:
        src = spec["ttf"]
        if spec.get("pin"):
            limits = {
                k: (tuple(v) if isinstance(v, list) else v)
                for k, v in spec["pin"].items()
            }
            pinned = instancer.instantiateVariableFont(
                TTFont(src, recalcTimestamp=False), limits
            )
            src = os.path.join(out_dir, "_pin_" + spec["slug"] + ".ttf")
            pinned.save(src)
        font = TTFont(src, recalcTimestamp=False)
        cmap = set(font.getBestCmap().keys())
        axes = [
            {
                "tag": a.axisTag,
                "min": a.minValue,
                "default": a.defaultValue,
                "max": a.maxValue,
            }
            for a in (font["fvar"].axes if "fvar" in font else [])
        ]
        files = []
        if spec.get("symbols"):
            count = symbol_data(font, spec["symbols"]["codepoints"], spec["symbols"]["out"])
            report.append(
                {
                    "slug": spec["slug"],
                    "ttf": spec["ttf"],
                    "pin": None,
                    "axes": axes,
                    "glyphs": len(font.getGlyphOrder()),
                    "icons": count,
                    "files": [],
                }
            )
            continue
        d = os.path.join(out_dir, spec["slug"])
        os.makedirs(d, exist_ok=True)
        full = os.path.join(d, "full.woff2")
        font.flavor = "woff2"
        font.save(full)
        files.append(
            {
                "subset": "full",
                "file": full,
                "bytes": os.path.getsize(full),
                "range": None,
            }
        )
        for name, rng in spec["ranges"].items():
            want = parse_ranges(rng)
            if not (want & cmap):
                continue
            path = os.path.join(d, f"{name}.woff2")
            subset_font(src, path, want)
            files.append(
                {
                    "subset": name,
                    "file": path,
                    "bytes": os.path.getsize(path),
                    "range": rng,
                }
            )
        report.append(
            {
                "slug": spec["slug"],
                "ttf": spec["ttf"],
                "pin": spec.get("pin"),
                "axes": axes,
                "glyphs": len(font.getGlyphOrder()),
                "cmap": merge_ranges(cmap),
                "files": files,
            }
        )
    json.dump(report, sys.stdout)


if __name__ == "__main__":
    main()
