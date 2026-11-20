// Pure hygiene rules shared by check.ts and its tests.

export interface Settings {
  permissions?: { deny?: unknown };
  enabledPlugins?: Record<string, boolean>;
  extraKnownMarketplaces?: Record<string, unknown>;
}

export const FORBIDDEN: Array<{ label: string; re: RegExp }> = [
  { label: "stale C:\\aphrody path", re: /C:[\\/]aphrody\b/i },
  { label: "stale gpui-os ref", re: /\bgpui-os\b/i },
  { label: "stale C--aphrody slug", re: /\bC--aphrody\b/ },
  { label: "stale bun-upstream-canary path", re: /bun-upstream-canary/i },
  { label: "hardcoded user home path", re: /C:[\\/]Users[\\/]aphro\b/i },
  { label: "deprecated pre-tool-guard", re: /pre-tool-guard/i },
  { label: "stale aphrody-skills ref", re: /\baphrody-skills\b/ },
  { label: "Sovereign slop wording", re: /\bsovereign\b/i },
];

/** Enabled plugins (`name@marketplace`) whose marketplace is neither the local `aphrody` one nor declared in settings. */
/** Marketplaces Claude Code ships built in: they need no extraKnownMarketplaces entry (`claude plugin marketplace list`: "Built in"). */
const BUILT_IN_MARKETPLACES = ["anthropic-plugin-directory"];

export function undeclaredPlugins(settings: Settings): string[] {
  const known = new Set([
    "aphrody",
    ...BUILT_IN_MARKETPLACES,
    ...Object.keys(settings.extraKnownMarketplaces ?? {}),
  ]);
  return Object.keys(settings.enabledPlugins ?? {}).filter(
    (id) => !known.has(id.split("@")[1] ?? ""),
  );
}
