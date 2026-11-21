"""material-cdn: check the live à-la-carte outputs by their bytes (glyph, ligature and symbol counts), not by status codes.

    uv run --quiet --with fonttools==4.68.0 --with brotli==1.2.1 tools/material-cdn/verify.py [BASE_URL]

Fetches icon fonts and text subsets, decodes the WOFF2 with fontTools and compares what the font holds with what the request named;
fetches a sprite and counts its <symbol>s. Prints one row per check (bytes, glyphs, ligatures, axes) and exits 1 on any mismatch.
"""

import io
import json
import re
import sys
import urllib.request

from fontTools.ttLib import TTFont

BASE = (sys.argv[1] if len(sys.argv) > 1 else "https://material.aphrody.com/v1").rstrip("/")
ICONS = ["home", "search", "settings", "menu", "close", "check", "add", "delete", "edit", "favorite"]
failures: list[str] = []


def fetch(path: str) -> tuple[bytes, dict[str, str]]:
    req = urllib.request.Request(f"{BASE}/{path}", headers={"User-Agent": "fw-material-verify", "Accept-Encoding": "identity"})
    with urllib.request.urlopen(req, timeout=60) as res:
        return res.read(), {k.lower(): v for k, v in res.headers.items()}


def ligatures(font: TTFont) -> int:
    count = 0
    if "GSUB" not in font:
        return 0
    for lookup in font["GSUB"].table.LookupList.Lookup:
        for sub in lookup.SubTable:
            sub = getattr(sub, "ExtSubTable", sub)
            if hasattr(sub, "ligatures"):
                count += sum(len(v) for v in sub.ligatures.values())
    return count


def check(label: str, ok: bool, detail: str) -> None:
    print(f"{'ok  ' if ok else 'FAIL'} {label:<44} {detail}")
    if not ok:
        failures.append(label)


def font_facts(data: bytes) -> TTFont:
    assert data[:4] == b"wOF2", "not a woff2"
    return TTFont(io.BytesIO(data))


def icon_font(label: str, query: str, icons: list[str], fills: bool) -> None:
    style = re.search(r"style=(\w+)", query).group(1)
    data, headers = fetch(f"icons.woff2?{query}")
    font = font_facts(data)
    table = json.loads(fetch(f"data/symbols/{style}.json")[0])
    index = {name: i for i, name in enumerate(table["names"])}
    chosen = {table["cp"][index[name]] for name in icons}
    letters = {c for name in icons for c in name}
    # Aliases (close/clear) share a glyph and keep their own ligature when every letter of the alias is in the subset.
    want_ligatures = sum(1 for name, cp in zip(table["names"], table["cp"]) if cp in chosen and set(name) <= letters)
    want_fill = len({table["fill"][index[name]] for name in icons} - {0}) if fills else 0
    glyphs = len(font.getGlyphOrder())
    want_glyphs = 1 + len(letters) + len(icons) + want_fill
    axes = [a.axisTag for a in font["fvar"].axes] if "fvar" in font else []
    check(
        label,
        glyphs == want_glyphs and ligatures(font) == want_ligatures and len(font.getBestCmap()) == len(letters) + len(icons),
        f"{len(data)} B, glyphs {glyphs}/{want_glyphs} (1 notdef + {len(letters)} letters + {len(icons)} icons + {want_fill} fill), ligatures {ligatures(font)}/{want_ligatures}, axes {axes or '-'}, {headers.get('x-material-source', '?')}",
    )


def main() -> None:
    icon_font("icons.woff2 10 icons, pinned axes", f"icons={','.join(ICONS)}&style=outlined", ICONS, False)
    icon_font("icons.woff2 10 icons, FILL 0..1", f"icons={','.join(ICONS)}&style=outlined&fill=0..1", ICONS, True)
    icon_font("icons.woff2 10 icons rounded, wght 100..700", f"icons={','.join(ICONS)}&style=rounded&wght=100..700&fill=0..1", ICONS, True)
    icon_font("icons.woff2 1 icon sharp", "icons=home&style=sharp", ["home"], False)

    data, _ = fetch(f"icons.svg?icons={','.join(ICONS)}&style=outlined&fill=0..1&ids=of")
    text = data.decode()
    symbols = re.findall(r'<symbol id="([of]-[a-z_]+)" viewBox="0 -960 960 960"><path d="[Mm]', text)
    check("icons.svg sprite o-/f- symbols", len(symbols) == 2 * len(ICONS) and f"o-{ICONS[0]}" in symbols, f"{len(data)} B, {len(symbols)} symbols")

    data, _ = fetch("icons.svg?icons=local_phone,call&style=outlined&ids=of")
    paths = re.findall(r'<symbol id="o-(?:local_phone|call)" viewBox="0 -960 960 960"><path d="([^"]+)"', data.decode())
    check("icons.svg alias local_phone = call", len(paths) == 2 and paths[0] == paths[1], f"{len(data)} B, {len(paths)} symbols")

    data, _ = fetch("fonts/google-sans-flex.woff2?text=Aphrody&wght=400..700&wdth=100")
    font = font_facts(data)
    cmap = sorted(font.getBestCmap())
    wanted = sorted({ord(c) for c in "Aphrody"})
    axes = {a.axisTag: (a.minValue, a.maxValue) for a in font["fvar"].axes}
    check(
        "google-sans-flex.woff2 text=Aphrody",
        cmap == wanted and axes == {"wght": (400.0, 700.0)},
        f"{len(data)} B, cmap {len(cmap)}/{len(wanted)} chars, glyphs {len(font.getGlyphOrder())}, axes {axes}",
    )

    data, _ = fetch("fonts/roboto-flex.woff2?unicodes=20-7E&wght=400&opsz=14&wdth=100&slnt=0&grad=0")
    font = font_facts(data)
    check("roboto-flex.woff2 U+20-7E, all axes pinned", len(font.getBestCmap()) == 95 and "fvar" not in font or not font["fvar"].axes, f"{len(data)} B, cmap {len(font.getBestCmap())}, static={'fvar' not in font}")

    css, _ = fetch("fonts/google-sans-flex.css?subsets=latin,latin-ext&wght=400..700")
    faces = css.decode().count("@font-face")
    check("google-sans-flex.css 2 subsets", faces == 2, f"{faces} @font-face")

    full, _ = fetch("index.json")
    check("index.json", b'"version"' in full, f"{len(full)} B")
    meta = json.loads(fetch("meta.json")[0])
    for slug, count in [("material-symbols-outlined", 4), ("material-symbols-rounded", 4), ("material-symbols-sharp", 4), ("google-sans-flex", 5), ("google-sans-code", 1), ("google-sans-code-italic", 1), ("roboto-flex", 13)]:
        axes = [a["tag"] for a in meta["families"][slug]["axes"]]
        check(f"meta.json {slug} axes", len(axes) == count, f"{len(axes)}: {' '.join(axes)}")

    if failures:
        print(f"\n{len(failures)} check(s) failed: {', '.join(failures)}")
        sys.exit(1)
    print("\nall checks passed")


if __name__ == "__main__":
    main()
