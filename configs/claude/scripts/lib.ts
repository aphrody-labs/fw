import {
  existsSync,
  lstatSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { homedir } from "node:os";
import { dirname, join, resolve } from "node:path";

export const REPO_ROOT = resolve(import.meta.dir, "..");
export const HOME = (
  process.env.USERPROFILE ??
  process.env.HOME ??
  homedir()
).replaceAll("\\", "/");

export function toSlash(p: string): string {
  return p.replaceAll("\\", "/");
}

/** Directory holding the checkouts: WORKSPACE_ROOT, else ~/src. Never derived from the working directory. */
function workspaceCandidates(): string[] {
  return [process.env.WORKSPACE_ROOT, join(HOME, "src")].filter(
    (x): x is string => Boolean(x),
  );
}

function resolveRepoDir(
  names: string[],
  markerFiles: string[],
): string | undefined {
  for (const r of workspaceCandidates()) {
    for (const name of names) {
      const dir = toSlash(join(r, name));
      if (!existsSync(dir)) continue;
      if (markerFiles.some((m) => existsSync(join(dir, m)))) return dir;
    }
  }
  return undefined;
}

export function discoverWorkspaceRoot(): string {
  const found = workspaceCandidates().find((d) => existsSync(d));
  return toSlash(resolve(found ?? join(HOME, "src")));
}

export function discoverSrcRoot(): string {
  const wsRoot = discoverWorkspaceRoot();
  const envSrc = process.env.SRC_ROOT
    ? toSlash(resolve(process.env.SRC_ROOT))
    : "";
  const isValidEnvSrc =
    envSrc &&
    envSrc.toLowerCase() !== wsRoot.toLowerCase() &&
    envSrc.length > 4 &&
    existsSync(envSrc);
  const candidates = [
    isValidEnvSrc ? envSrc : undefined,
    resolveRepoDir(
      ["aphrody"],
      ["product.toml", "crates/os_desktop/Cargo.toml"],
    ),
    resolveRepoDir(["ai-os"], ["frameworks/ai-os", "tools/identity.toml"]),
    resolveRepoDir(["ay"], ["tools/identity.toml", "frameworks/ai-os"]),
  ].filter((x): x is string => Boolean(x));

  if (candidates.length > 0) return toSlash(candidates[0]!);
  return toSlash(join(wsRoot, "aphrody"));
}

export function discoverWorkspaces(): Record<string, string> {
  const wsRoot = discoverWorkspaceRoot();
  const map: Record<string, string> = {};
  if (existsSync(wsRoot)) {
    map[wsRoot] = "src";
    try {
      for (const entry of readdirSync(wsRoot, { withFileTypes: true })) {
        if (!entry.isDirectory() || entry.name.startsWith(".")) continue;
        const full = toSlash(join(wsRoot, entry.name));
        if (
          existsSync(join(full, ".git")) ||
          existsSync(join(full, "product.toml")) ||
          existsSync(join(full, "Cargo.toml")) ||
          existsSync(join(full, "package.json"))
        ) {
          const slug =
            entry.name
              .toLowerCase()
              .replace(/[^a-z0-9-]+/g, "-")
              .replace(/^-+|-+$/g, "") || "repo";
          map[full] = slug;
        }
      }
    } catch {
      // Ignore unreadable workspace entries.
    }
  }
  map[toSlash(REPO_ROOT)] = "claude";
  return map;
}

export function writeIfChanged(path: string, content: string): boolean {
  const normalized = content;
  if (existsSync(path)) {
    try {
      if (readFileSync(path, "utf8") === normalized) return false;
    } catch {
      // Rewrite if unreadable.
    }
  }
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, normalized, "utf8");
  return true;
}

export function ensureDirLink(target: string, linkPath: string): void {
  const absTarget = resolve(target);
  const absLink = resolve(linkPath);
  mkdirSync(dirname(absLink), { recursive: true });
  try {
    const st = lstatSync(absLink);
    if (st.isSymbolicLink()) {
      rmSync(absLink, { recursive: true, force: true });
    } else if (st.isDirectory()) {
      rmSync(absLink, { recursive: true, force: true });
    } else {
      rmSync(absLink, { force: true });
    }
  } catch {
    // Not present yet.
  }
  symlinkSync(
    absTarget,
    absLink,
    process.platform === "win32" ? "junction" : "dir",
  );
}

export function runCmd(
  cmd: string[],
  cwd = REPO_ROOT,
): { ok: boolean; code: number; out: string; err: string } {
  const r = Bun.spawnSync(cmd, {
    cwd,
    stdout: "pipe",
    stderr: "pipe",
    stdin: "ignore",
  });
  return {
    ok: r.exitCode === 0,
    code: r.exitCode,
    out: r.stdout.toString().trim(),
    err: r.stderr.toString().trim(),
  };
}

export function templateVars(): Record<string, string> {
  const srcRoot = discoverSrcRoot();
  const wsRoot = discoverWorkspaceRoot();
  return {
    HOME,
    HOME_WIN: HOME.replaceAll("/", "\\"),
    REPO_ROOT: toSlash(REPO_ROOT),
    SRC_ROOT: srcRoot,
    SRC_ROOT_WIN: srcRoot.replaceAll("/", "\\"),
    WORKSPACE_ROOT: wsRoot,
    WORKSPACE_ROOT_WIN: wsRoot.replaceAll("/", "\\"),
    RELAY_URL: process.env.RELAY_URL ?? "http://127.0.0.1:8790",
  };
}

export function renderTemplate(
  raw: string,
  vars = templateVars(),
  jsonEscape = false,
): string {
  let out = raw;
  for (const [k, v] of Object.entries(vars)) {
    const val = jsonEscape ? v.replaceAll("\\", "\\\\") : v;
    out = out.replaceAll(`\${${k}}`, val);
  }
  return out;
}
