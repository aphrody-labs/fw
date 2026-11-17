// Where machine- and work-specific data lives. Tracked files hold code, schemas, specs and standards only; everything that names
// this machine, the fleet or the state of the work stays out of git:
//   ay.local.toml  (git-ignored) overrides and extends ay.toml: host aliases, host paths, local checkout hints
//   .private/      (git-ignored) generated reports, telemetry, research notes, the work ledger and the plan page
// AY_PRIVATE_DIR moves the second one (for example to a directory outside the checkout).
import { existsSync, mkdirSync, readFileSync } from "node:fs";
import { dirname, isAbsolute, join, resolve } from "node:path";

export const repoRoot = resolve(import.meta.dir, "..");

export const privateDir = (root = repoRoot): string => {
  const dir = Bun.env.AY_PRIVATE_DIR;
  return dir ? (isAbsolute(dir) ? dir : resolve(root, dir)) : join(root, ".private");
};

/** Path inside the private directory; the parent directory is created when `forWrite` is set. */
export const privatePath = (rel: string, root = repoRoot, forWrite = false): string => {
  const path = join(privateDir(root), rel);
  if (forWrite) mkdirSync(dirname(path), { recursive: true });
  return path;
};

type Json = Record<string, unknown>;
const isTable = (v: unknown): v is Json => typeof v === "object" && v !== null && !Array.isArray(v);

/** Deep merge: tables merge key by key, every other value (arrays included) is replaced by the overlay. */
export function merge(base: Json, overlay: Json): Json {
  const out: Json = { ...base };
  for (const [k, v] of Object.entries(overlay)) out[k] = isTable(v) && isTable(out[k]) ? merge(out[k] as Json, v) : v;
  return out;
}

/** ay.toml merged with the optional git-ignored ay.local.toml. */
export function readAy(root = repoRoot): Json {
  const base = Bun.TOML.parse(readFileSync(join(root, "ay.toml"), "utf8")) as Json;
  const local = join(root, "ay.local.toml");
  return existsSync(local) ? merge(base, Bun.TOML.parse(readFileSync(local, "utf8")) as Json) : base;
}
