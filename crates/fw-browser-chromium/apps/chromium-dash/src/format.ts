/** Language of the pages: BCP 47, source baseline English, one value for the document `lang` and the CLDR formatters. */
export const LANG = process.env.CHROMIUM_DASH_LANG ?? "en";

/** Base direction of `LANG` from the script CLDR assigns to it. */
export const DIR = ["Arab", "Hebr", "Thaa", "Syrc", "Adlm", "Nkoo"].includes(new Intl.Locale(LANG).maximize().script ?? "") ? "rtl" : "ltr";

/** Dates and numbers the way both dashboards show them (UTC, so pages are the same for every reader). */
const DATE = new Intl.DateTimeFormat(LANG, { month: "short", day: "numeric", year: "numeric", timeZone: "UTC" });
const DAY = new Intl.DateTimeFormat(LANG, { weekday: "short", month: "short", day: "numeric", year: "numeric", timeZone: "UTC" });

export const dateOf = (ms: number) => DATE.format(ms);

/** `2026-11-17T00:00:00` as `Tue, Nov 17, 2026`. */
export function scheduleDay(date: string | undefined): string {
  return date ? DAY.format(Date.parse(`${date}Z`)) : "";
}

/** `2026-11-16 14:23:24.535593` (Chrome Status, UTC) as `Nov 16, 2026`. */
export function statusDate(when: string | undefined): string {
  return when ? DATE.format(Date.parse(`${when.replace(" ", "T").slice(0, 19)}Z`)) : "";
}

/** A day-granular relative time: `2 days ago`, `yesterday`, `today`, `tomorrow`, `in 5 days`. */
export function ago(ms: number, now = Date.now()): string {
  const days = Math.round((now - ms) / 86_400_000);
  if (days === 0) return "today";
  if (days === 1) return "yesterday";
  if (days === -1) return "tomorrow";
  return days > 0 ? `${days} days ago` : `in ${-days} days`;
}

/** The label of a launch milestone cell: `158`, or `Proposed` when no milestone is set. */
export const milestoneLabel = (m: number | null | undefined) => (m ? String(m) : "");
