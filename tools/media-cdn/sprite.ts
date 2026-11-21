// SVG files to the <symbol>s of one sprite.

/** One <symbol> from an SVG file: its viewBox and inner markup. Scripts, event handlers and external references are refused. */
export function symbolOf(id: string, svg: string): string {
  const root = /<svg\b([^>]*)>([\s\S]*)<\/svg>\s*$/i.exec(svg.replace(/<\?xml[\s\S]*?\?>|<!--[\s\S]*?-->|<!DOCTYPE[\s\S]*?>/gi, ""));
  if (!root) throw new Error(`${id}: not an SVG document`);
  const attributes = root[1]!;
  const inner = root[2]!.trim();
  if (/<script|\son[a-z]+\s*=|<foreignObject|javascript:|href\s*=\s*["'](?!#)/i.test(inner)) throw new Error(`${id}: scripts, event handlers and external references are not allowed in a sprite`);
  const size = (name: string) => /^\d+(?:\.\d+)?(?:px)?$/.exec(new RegExp(`\\s${name}\\s*=\\s*["']([^"']+)["']`, "i").exec(attributes)?.[1] ?? "")?.[0]?.replace("px", "");
  // Icons without a viewBox are drawn in the box given by their width and height.
  const viewBox = /viewBox\s*=\s*["']([^"']+)["']/i.exec(attributes)?.[1] ?? (size("width") && size("height") ? `0 0 ${size("width")} ${size("height")}` : undefined);
  if (!viewBox) throw new Error(`${id}: the SVG has no viewBox and no width and height`);
  return `<symbol id="${id}" viewBox="${viewBox}">${inner}</symbol>`;
}
