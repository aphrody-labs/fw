/** A tiny escaping HTML template: interpolated values are escaped unless wrapped with `raw` or produced by `html`. */
export class Safe {
  constructor(readonly value: string) {}
  toString() {
    return this.value;
  }
}

export const raw = (s: string) => new Safe(s);

export const escape = (s: unknown): string =>
  String(s ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");

type Value = Safe | string | number | boolean | null | undefined | Value[];

function render(v: Value): string {
  if (v instanceof Safe) return v.value;
  if (Array.isArray(v)) return v.map(render).join("");
  if (v === false || v === null || v === undefined) return "";
  return escape(v);
}

export function html(strings: TemplateStringsArray, ...values: Value[]): Safe {
  let out = strings[0]!;
  values.forEach((v, i) => (out += render(v) + strings[i + 1]!));
  return new Safe(out);
}
