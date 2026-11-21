// Just enough SFNT reading to count what a subset kept: glyphs (maxp), cmap entries and GSUB ligatures.
export interface FontFacts {
  glyphs: number;
  codepoints: number[];
  ligatures: number;
  axes: string[];
}

const u16 = (v: DataView, o: number) => v.getUint16(o);
const u32 = (v: DataView, o: number) => v.getUint32(o);

export function readSfnt(bytes: Uint8Array): FontFacts {
  const v = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const tables = new Map<string, number>();
  for (let i = 0; i < u16(v, 4); i++) {
    const o = 12 + i * 16;
    tables.set(String.fromCharCode(...bytes.slice(o, o + 4)), u32(v, o + 8));
  }
  const at = (name: string) => tables.get(name);
  const glyphs = u16(v, at("maxp")! + 4);

  const codepoints: number[] = [];
  const cmap = at("cmap")!;
  for (let i = 0; i < u16(v, cmap + 2); i++) {
    const sub = cmap + u32(v, cmap + 4 + i * 8 + 4);
    const format = u16(v, sub);
    if (format === 12) {
      for (let g = 0; g < u32(v, sub + 12); g++) {
        const o = sub + 16 + g * 12;
        for (let c = u32(v, o); c <= u32(v, o + 4); c++) codepoints.push(c);
      }
    } else if (format === 4) {
      const segments = u16(v, sub + 6) / 2;
      for (let s = 0; s < segments; s++) {
        const end = u16(v, sub + 14 + s * 2);
        const start = u16(v, sub + 16 + segments * 2 + s * 2);
        for (let c = start; c <= end && c !== 0xffff; c++) codepoints.push(c);
      }
    }
  }

  let ligatures = 0;
  const gsub = at("GSUB");
  if (gsub !== undefined) {
    const lookupList = gsub + u16(v, gsub + 8);
    for (let l = 0; l < u16(v, lookupList); l++) {
      const lookup = lookupList + u16(v, lookupList + 2 + l * 2);
      let type = u16(v, lookup);
      for (let s = 0; s < u16(v, lookup + 4); s++) {
        let sub = lookup + u16(v, lookup + 6 + s * 2);
        if (type === 7) {
          type = u16(v, sub + 2);
          sub += u32(v, sub + 4);
        }
        if (type !== 4) continue;
        const sets = u16(v, sub + 4);
        for (let k = 0; k < sets; k++) ligatures += u16(v, sub + u16(v, sub + 6 + k * 2));
      }
    }
  }

  const axes: string[] = [];
  const fvar = at("fvar");
  if (fvar !== undefined) {
    const offset = u16(v, fvar + 4);
    for (let a = 0; a < u16(v, fvar + 8); a++) {
      const o = fvar + offset + a * u16(v, fvar + 10);
      axes.push(String.fromCharCode(...bytes.slice(o, o + 4)));
    }
  }
  return { glyphs, codepoints: [...new Set(codepoints)].sort((x, y) => x - y), ligatures, axes };
}
