#!/usr/bin/env bun
// Flags hardcoded absolute-path literals (Windows drives, MSYS /c/..., /home/<user>, /root, /srv, /Users/<name>) in the
// tracked files of a repository. Paths must be derived (import.meta.dir, git rev-parse --show-toplevel, os.homedir(),
// XDG and known folders, WORKSPACE_ROOT, per-host tables in ay.toml), not written down.
//   bun tools/paths.ts                  scan the repo containing the current directory
//   bun tools/paths.ts --root <dir>     scan another repository (git ls-files), e.g. a sibling checkout
//   bun tools/paths.ts --include <path> only these files or directories of the repository (repeatable), for very large trees
//   bun tools/paths.ts --allow <file>   allowlist (default: <root>/paths.allow, else <root>/tools/paths.allow)
//   bun tools/paths.ts --summary        counts only (per file and total)
//   bun tools/paths.ts --json           machine-readable result
// Exit code 1 when a finding is not allowlisted.
// Allowlist: one repo-relative glob per line ("# reason" after the glob is a comment); a line carrying `paths:ok` is exempt.
import { existsSync, readFileSync, statSync } from "node:fs";
import { join, resolve } from "node:path";

export type Finding = { file: string; line: number; kind: Kind; match: string; text: string };
export type Kind = "drive" | "msys" | "home" | "users" | "root" | "srv";

const RULES: { kind: Kind; re: RegExp }[] = [
  // A drive letter with a backslash, escaped backslash or slash. A leading identifier character means it is not a drive,
  // and a lowercase letter before `\n`, `\t`... is a label followed by an escape sequence.
  { kind: "drive", re: /(?<![A-Za-z0-9_])(?:[A-Za-z]:(?:\\\\|\/)|[A-Z]:\\|[a-z]:\\(?![nrtbfv0]))[A-Za-z0-9$%._{(]/g },
  { kind: "msys", re: /(?<![A-Za-z0-9_.:\-/~])\/[cd]\/[A-Za-z]/g },
  { kind: "home", re: /(?<![A-Za-z0-9_.\-])\/home\/[A-Za-z_][\w.-]*/g },
  { kind: "users", re: /(?<![A-Za-z0-9_.\-])\/Users\/[A-Za-z_][\w.-]*/g },
  { kind: "root", re: /(?<![A-Za-z0-9_.\-])\/root\//g },
  { kind: "srv", re: /(?<![A-Za-z0-9_.\-])\/srv\//g },
];

const BINARY = /\.(png|jpe?g|gif|webp|avif|ico|icns|bmp|svgz|woff2?|ttf|otf|eot|pdf|zip|gz|tgz|xz|zst|bz2|7z|tar|wasm|exe|dll|so|dylib|a|lib|o|obj|rlib|pdb|node|mp[34]|mov|webm|ogg|wav|flac|bin|dat|sqlite3?|db|parquet|lock|lockb|snap|map|min\.js)$/i;
const MAX_BYTES = 1 << 20;

/** Findings in one text. Lines carrying `paths:ok` are exempt. */
export function scanText(file: string, text: string): Finding[] {
  const out: Finding[] = [];
  const lines = text.split("\n");
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i]!;
    if (line.includes("paths:ok")) continue;
    for (const { kind, re } of RULES) {
      re.lastIndex = 0;
      for (const m of line.matchAll(re)) out.push({ file, line: i + 1, kind, match: m[0], text: line.trim().slice(0, 160) });
    }
  }
  return out;
}

/** Glob (`*` within a segment, `**` across segments, `?`) to a RegExp over repo-relative `/` paths. */
export function globToRegExp(glob: string): RegExp {
  let g = glob.replaceAll("\\", "/").replace(/^\.\//, "");
  if (g.endsWith("/")) g += "**";
  if (!g.includes("/")) g = `**/${g}`;
  let re = "";
  for (let i = 0; i < g.length; i++) {
    const c = g[i]!;
    if (c === "*") {
      if (g[i + 1] === "*") {
        i++;
        if (g[i + 1] === "/") {
          i++;
          re += "(?:.*/)?";
        } else re += ".*";
      } else re += "[^/]*";
    } else if (c === "?") re += "[^/]";
    else re += c.replace(/[.+^${}()|[\]\\]/g, "\\$&");
  }
  return new RegExp(`^${re}$`);
}

export function parseAllow(text: string): RegExp[] {
  return text
    .split("\n")
    .map(l => l.replace(/\s+#.*$/, "").trim())
    .filter(l => l && !l.startsWith("#"))
    .map(globToRegExp);
}

function git(root: string, args: string[]): string {
  const p = Bun.spawnSync(["git", "-C", root, ...args], { stdout: "pipe", stderr: "pipe" });
  if (p.exitCode !== 0) throw new Error(`git ${args.join(" ")}: ${p.stderr.toString().trim()}`);
  return p.stdout.toString();
}

/** Findings of every tracked text file under `root` that the allowlist does not exempt. */
export function scanRepo(root: string, allow: RegExp[] = [], include: string[] = []): Finding[] {
  const files = git(root, ["ls-files", "-z", "--cached", "--exclude-standard", ...(include.length ? ["--", ...include] : [])])
    .split("\0")
    .filter(f => f && !BINARY.test(f) && !allow.some(r => r.test(f)));
  const out: Finding[] = [];
  for (const f of files) {
    const abs = join(root, f);
    try {
      const st = statSync(abs);
      if (!st.isFile() || st.size > MAX_BYTES) continue;
      const text = readFileSync(abs, "utf8");
      if (text.includes("\0")) continue;
      out.push(...scanText(f, text));
    } catch {
      // Submodule gitlinks and files deleted in the working tree are not scanned.
    }
  }
  return out;
}

export function allowFile(root: string, explicit?: string): string | undefined {
  const candidates = explicit ? [resolve(explicit)] : [join(root, "paths.allow"), join(root, "tools", "paths.allow")];
  return candidates.find(existsSync);
}

if (import.meta.main) {
  const args = process.argv.slice(2);
  const opt = (name: string) => {
    const i = args.indexOf(name);
    return i >= 0 ? args[i + 1] : undefined;
  };
  const root = resolve(opt("--root") ?? git(process.cwd(), ["rev-parse", "--show-toplevel"]).trim());
  const af = allowFile(root, opt("--allow"));
  const include = args.flatMap((a, i) => (a === "--include" && args[i + 1] ? [args[i + 1]!] : []));
  const findings = scanRepo(root, af ? parseAllow(readFileSync(af, "utf8")) : [], include);
  if (args.includes("--json")) console.log(JSON.stringify(findings, null, 2));
  else if (args.includes("--summary")) {
    const per = new Map<string, number>();
    for (const f of findings) per.set(f.file, (per.get(f.file) ?? 0) + 1);
    for (const [f, n] of [...per].sort((a, b) => b[1] - a[1])) console.log(`${String(n).padStart(5)}  ${f}`);
    console.log(`${findings.length} absolute-path literal(s) in ${per.size} file(s) of ${root}`);
  } else {
    for (const f of findings) console.log(`${f.file}:${f.line}: [${f.kind}] ${f.match}  ${f.text}`);
    console.log(`${findings.length} absolute-path literal(s) in ${new Set(findings.map(f => f.file)).size} file(s) of ${root}`);
  }
  process.exit(findings.length ? 1 : 0);
}
