import catalog from "../locales.toml";

// Messages: locales.toml has one [locales.<tag>.messages] table per language (the shape of aphrody/assets/os/locales.toml that
// fw-theme Catalog reads). FW_LOCALE, then LANG, picks the language; a missing message falls back to the default locale, then to its id.
type Messages = Record<string, string>;
const locales = catalog.locales as unknown as { default_locale: string } & Record<string, { messages: Messages }>;
const requested = (process.env.FW_LOCALE ?? process.env.LANG ?? "").split(".")[0].replace("_", "-");
const fallback = locales[locales.default_locale]?.messages ?? {};
const messages = locales[requested]?.messages ?? locales[requested.split("-")[0]]?.messages ?? fallback;
export const l10n = (id: string): string => messages[id] ?? fallback[id] ?? id;
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
