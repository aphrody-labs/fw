#!/usr/bin/env bun
// Module/dependency graph of a config or agent repo: packages, skills, hooks, MCP servers, scripts, scheduled jobs.
// Output: docs/graph/<name>.json (full) and docs/graph/<name>.mmd (Mermaid, skills collapsed per package).
//   bun scripts/graph.ts                 graph of this repo (name "claude")
//   bun scripts/graph.ts <dir> <name>    graph of another checkout, e.g. an openclaw or hermes-agent clone
import {
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { dirname, join, posix, relative } from "node:path";
import { REPO_ROOT, toSlash } from "./lib.ts";

export type NodeKind =
  | "package"
  | "skill"
  | "hook"
  | "mcp"
  | "script"
  | "agent"
  | "cron"
  | "plugin"
  | "module";
export type EdgeKind =
  "contains" | "declares" | "runs" | "imports" | "enables" | "depends";

export interface GNode {
  id: string;
  kind: NodeKind;
  label: string;
  path?: string;
}

export interface GEdge {
  from: string;
  to: string;
  kind: EdgeKind;
}

export interface Graph {
  name: string;
  nodes: GNode[];
  edges: GEdge[];
}

const SKIP = new Set([
  "node_modules",
  ".git",
  "plugins",
  "projects",
  "learning",
  "tmp",
  "dist",
  "target",
  "__pycache__",
]);

function read(file: string): string {
  return readFileSync(file, "utf8");
}

function json<T>(file: string): T | null {
  try {
    return JSON.parse(read(file)) as T;
  } catch {
    return null;
  }
}

export function walk(
  dir: string,
  accept: (name: string) => boolean,
  depth = 6,
): string[] {
  if (!existsSync(dir) || depth < 0) return [];
  const out: string[] = [];
  for (const e of readdirSync(dir, { withFileTypes: true })) {
    if (SKIP.has(e.name)) continue;
    const p = join(dir, e.name);
    if (e.isDirectory()) out.push(...walk(p, accept, depth - 1));
    else if (accept(e.name)) out.push(p);
  }
  return out;
}

/** Relative module specifiers of a TypeScript file: static, re-export and dynamic imports. */
export function importsOf(source: string): string[] {
  const out = new Set<string>();
  for (const m of source.matchAll(
    /(?:from|import)\s*\(?\s*["'](\.{1,2}\/[^"']+)["']/g,
  ))
    out.add(m[1]!);
  return [...out];
}

export function resolveImport(fromFile: string, spec: string): string {
  return posix.normalize(posix.join(posix.dirname(fromFile), spec));
}

/** The repo-relative script paths a command line refers to (`${CLAUDE_PLUGIN_ROOT}` already substituted by the caller). */
export function scriptsInCommand(command: string): string[] {
  return [...command.matchAll(/([\w./${}-]+\.(?:ts|js|py|sh))\b/g)].map(
    (m) => m[1]!,
  );
}

interface HookGroup {
  matcher?: string;
  hooks?: Array<{ command?: string }>;
}

export function buildGraph(root: string, name: string): Graph {
  const nodes = new Map<string, GNode>();
  const edges: GEdge[] = [];
  const add = (n: GNode) => nodes.set(n.id, n);
  const edge = (from: string, to: string, kind: EdgeKind) => {
    if (!edges.some((e) => e.from === from && e.to === to && e.kind === kind))
      edges.push({ from, to, kind });
  };
  const rel = (p: string) => toSlash(relative(root, p));

  // Scripts and hook handlers: every TypeScript file, with relative import edges.
  const tsFiles = walk(
    root,
    (n) =>
      /\.(ts|mts)$/.test(n) && !n.endsWith(".test.ts") && !n.endsWith(".d.ts"),
  );
  for (const f of tsFiles)
    add({
      id: `script:${rel(f)}`,
      kind: "script",
      label: posix.basename(rel(f)),
      path: rel(f),
    });
  for (const f of tsFiles) {
    for (const spec of importsOf(read(f))) {
      const target = resolveImport(rel(f), spec);
      if (nodes.has(`script:${target}`))
        edge(`script:${rel(f)}`, `script:${target}`, "imports");
    }
  }

  // Packages (plugins), their skills, agents, hooks and MCP servers.
  const pkgRoot = join(root, "packages");
  const pkgs = existsSync(pkgRoot)
    ? readdirSync(pkgRoot).filter((d) =>
        statSync(join(pkgRoot, d)).isDirectory(),
      )
    : [];
  for (const p of pkgs) {
    const dir = join(pkgRoot, p);
    const pid = `package:${p}`;
    const manifest = json<{ version?: string }>(
      join(dir, ".claude-plugin", "plugin.json"),
    );
    add({
      id: pid,
      kind: "package",
      label: `${p}${manifest?.version ? ` ${manifest.version}` : ""}`,
      path: rel(dir),
    });

    for (const f of walk(join(dir, "skills"), (n) => n === "SKILL.md", 3)) {
      const sid = `skill:${p}/${posix.basename(posix.dirname(toSlash(f)))}`;
      add({
        id: sid,
        kind: "skill",
        label: posix.basename(posix.dirname(toSlash(f))),
        path: rel(f),
      });
      edge(pid, sid, "contains");
    }
    for (const f of walk(join(dir, "agents"), (n) => n.endsWith(".md"), 1)) {
      const aid = `agent:${p}/${posix.basename(toSlash(f), ".md")}`;
      add({
        id: aid,
        kind: "agent",
        label: posix.basename(toSlash(f), ".md"),
        path: rel(f),
      });
      edge(pid, aid, "contains");
    }

    const hooks =
      json<{ hooks?: Record<string, HookGroup[]> }>(
        join(dir, ".claude-plugin", "hooks.json"),
      )?.hooks ?? {};
    for (const [event, groups] of Object.entries(hooks)) {
      for (const g of groups) {
        for (const h of g.hooks ?? []) {
          if (!h.command) continue;
          const hid = `hook:${p}/${event}${g.matcher ? `[${g.matcher}]` : ""}/${scriptsInCommand(h.command)[0]?.split("/").pop() ?? "cmd"}`;
          add({
            id: hid,
            kind: "hook",
            label: `${event}${g.matcher ? ` ${g.matcher}` : ""}`,
          });
          edge(pid, hid, "declares");
          for (const s of scriptsInCommand(h.command)) {
            const target = s.replace("${CLAUDE_PLUGIN_ROOT}", rel(dir));
            if (nodes.has(`script:${target}`))
              edge(hid, `script:${target}`, "runs");
          }
        }
      }
    }

    const mcp =
      json<{
        mcpServers?: Record<
          string,
          { command?: string; args?: string[]; url?: string; type?: string }
        >;
      }>(join(dir, ".claude-plugin", "mcp.json"))?.mcpServers ?? {};
    for (const [server, cfg] of Object.entries(mcp)) {
      const mid = `mcp:${p}/${server}`;
      add({
        id: mid,
        kind: "mcp",
        label: `${server}${cfg.url ? " (remote)" : ""}`,
      });
      edge(pid, mid, "declares");
      for (const a of cfg.args ?? []) {
        const target = a.replace("${CLAUDE_PLUGIN_ROOT}", rel(dir));
        if (nodes.has(`script:${target}`))
          edge(mid, `script:${target}`, "runs");
      }
    }
  }

  // Standalone skills (repo skills/ directory, not tied to a plugin).
  for (const f of walk(join(root, "skills"), (n) => n === "SKILL.md", 3)) {
    const sid = `skill:repo/${posix.basename(posix.dirname(toSlash(f)))}`;
    add({
      id: sid,
      kind: "skill",
      label: posix.basename(posix.dirname(toSlash(f))),
      path: rel(f),
    });
  }

  // Enabled external plugins from settings.json.
  const settings = json<{ enabledPlugins?: Record<string, boolean> }>(
    join(root, "settings.json"),
  );
  for (const [id, on] of Object.entries(settings?.enabledPlugins ?? {})) {
    if (!on) continue;
    const [pname] = id.split("@");
    const local = nodes.has(`package:${pname}`) && id.endsWith("@aphrody");
    if (!local) add({ id: `plugin:${id}`, kind: "plugin", label: id });
    else edge("settings", `package:${pname}`, "enables");
    if (!local) edge("settings", `plugin:${id}`, "enables");
  }
  if (settings)
    add({
      id: "settings",
      kind: "package",
      label: "settings.json",
      path: "settings.json",
    });

  // Scheduled jobs of agent.toml.
  if (existsSync(join(root, "agent.toml"))) {
    const toml = Bun.TOML.parse(read(join(root, "agent.toml"))) as {
      cron?: Array<{ id: string; argv?: string[]; kind: string }>;
    };
    for (const j of toml.cron ?? []) {
      const cid = `cron:${j.id}`;
      add({ id: cid, kind: "cron", label: j.id });
      edge("agent.toml", cid, "declares");
      for (const a of j.argv ?? [])
        if (nodes.has(`script:${a}`)) edge(cid, `script:${a}`, "runs");
    }
    add({
      id: "agent.toml",
      kind: "package",
      label: "agent.toml",
      path: "agent.toml",
    });
  }

  return {
    name,
    nodes: [...nodes.values()].sort((a, b) => a.id.localeCompare(b.id)),
    edges: edges.sort((a, b) =>
      `${a.from}${a.to}`.localeCompare(`${b.from}${b.to}`),
    ),
  };
}

interface PkgJson {
  name?: string;
  version?: string;
  dependencies?: Record<string, string>;
  devDependencies?: Record<string, string>;
  peerDependencies?: Record<string, string>;
}

/** pnpm/npm workspace: every package.json under packages/, extensions/, apps/ is a node; dependencies between them are edges. */
export function buildJsWorkspaceGraph(root: string, name: string): Graph {
  const nodes: GNode[] = [];
  const byName = new Map<string, string>();
  const manifests: Array<{ id: string; pkg: PkgJson }> = [];
  for (const group of ["packages", "extensions", "apps", "ui", "tools"]) {
    const dir = join(root, group);
    if (!existsSync(dir)) continue;
    const files =
      group === "ui"
        ? [join(dir, "package.json")]
        : readdirSync(dir).map((d) => join(dir, d, "package.json"));
    for (const f of files.filter(existsSync)) {
      const pkg = json<PkgJson>(f);
      if (!pkg) continue;
      const rel = toSlash(relative(root, dirname(f)));
      const id = `${group === "extensions" ? "plugin" : "package"}:${rel}`;
      nodes.push({
        id,
        kind: group === "extensions" ? "plugin" : "package",
        label: pkg.name ?? rel,
        path: rel,
      });
      if (pkg.name) byName.set(pkg.name, id);
      manifests.push({ id, pkg });
    }
  }
  const edges: GEdge[] = [];
  for (const { id, pkg } of manifests) {
    for (const dep of Object.keys({
      ...pkg.dependencies,
      ...pkg.devDependencies,
      ...pkg.peerDependencies,
    })) {
      const to = byName.get(dep);
      if (to && to !== id) edges.push({ from: id, to, kind: "depends" });
    }
  }
  for (const f of walk(root, (n) => n === "SKILL.md", 4)) {
    const rel = toSlash(relative(root, f));
    nodes.push({
      id: `skill:${rel}`,
      kind: "skill",
      label: posix.basename(posix.dirname(rel)),
      path: rel,
    });
  }
  return {
    name,
    nodes: nodes.sort((a, b) => a.id.localeCompare(b.id)),
    edges: edges.sort((a, b) =>
      `${a.from}${a.to}`.localeCompare(`${b.from}${b.to}`),
    ),
  };
}

/** Python repo: top-level packages/modules are nodes, edges are import counts between them (collapsed to one edge per pair). */
export function buildPythonGraph(root: string, name: string): Graph {
  const tops = new Set<string>();
  for (const e of readdirSync(root, { withFileTypes: true })) {
    if (
      SKIP.has(e.name) ||
      e.name.startsWith(".") ||
      e.name === "tests" ||
      e.name === "tests-js"
    )
      continue;
    if (
      e.isDirectory() &&
      walk(join(root, e.name), (n) => n.endsWith(".py"), 2).length > 0
    )
      tops.add(e.name);
    else if (e.isFile() && e.name.endsWith(".py"))
      tops.add(e.name.slice(0, -3));
  }
  const counts = new Map<string, number>();
  const files = walk(root, (n) => n.endsWith(".py"), 6).filter(
    (f) => !/[\\/]tests?[\\/]/.test(f),
  );
  for (const f of files) {
    const rel = toSlash(relative(root, f));
    const from = rel.includes("/") ? rel.split("/")[0]! : rel.slice(0, -3);
    if (!tops.has(from)) continue;
    for (const m of read(f).matchAll(/^(?:from|import)\s+([A-Za-z_][\w]*)/gm)) {
      const to = m[1]!;
      if (to !== from && tops.has(to))
        counts.set(`${from}\0${to}`, (counts.get(`${from}\0${to}`) ?? 0) + 1);
    }
  }
  const nodes: GNode[] = [...tops].map((t) => ({
    id: `module:${t}`,
    kind: "module",
    label: t,
    path: t,
  }));
  for (const f of walk(root, (n) => n === "SKILL.md", 4)) {
    const rel = toSlash(relative(root, f));
    nodes.push({
      id: `skill:${rel}`,
      kind: "skill",
      label: posix.basename(posix.dirname(rel)),
      path: rel,
    });
  }
  for (const f of walk(join(root, "plugins"), (n) => n === "plugin.yaml", 3)) {
    const rel = toSlash(relative(root, f));
    nodes.push({
      id: `plugin:${posix.dirname(rel)}`,
      kind: "plugin",
      label: posix.dirname(rel),
      path: rel,
    });
  }
  const edges: GEdge[] = [...counts.keys()].map((k) => ({
    from: `module:${k.split("\0")[0]}`,
    to: `module:${k.split("\0")[1]}`,
    kind: "imports" as const,
  }));
  return {
    name,
    nodes: nodes.sort((a, b) => a.id.localeCompare(b.id)),
    edges: edges.sort((a, b) =>
      `${a.from}${a.to}`.localeCompare(`${b.from}${b.to}`),
    ),
  };
}

/** Picks the builder from the repo shape: Python project (checked first, Hermes also carries a pnpm workspace for its UIs), pnpm workspace, or the Claude config layout. */
export function buildAny(root: string, name: string): Graph {
  if (existsSync(join(root, "pyproject.toml")))
    return buildPythonGraph(root, name);
  if (existsSync(join(root, "pnpm-workspace.yaml")))
    return buildJsWorkspaceGraph(root, name);
  return buildGraph(root, name);
}

const mmId = (id: string) => id.replace(/[^A-Za-z0-9]/g, "_");
const mmLabel = (s: string) => s.replaceAll('"', "'");

/** Mermaid flowchart. Skills collapse into one counted node per package so the diagram stays readable. */
export function toMermaid(g: Graph): string {
  const lines = ["flowchart LR"];
  const skillCount = new Map<string, number>();
  for (const e of g.edges)
    if (e.kind === "contains" && e.to.startsWith("skill:"))
      skillCount.set(e.from, (skillCount.get(e.from) ?? 0) + 1);
  const shape: Record<NodeKind, [string, string]> = {
    package: ["[[", "]]"],
    skill: ["(", ")"],
    hook: ["{{", "}}"],
    mcp: ["[(", ")]"],
    script: ["[", "]"],
    agent: ["((", "))"],
    cron: [">", "]"],
    plugin: ["[/", "/]"],
    module: ["[", "]"],
  };
  const collapsePlugins =
    g.nodes.filter((n) => n.kind === "plugin").length > 40;
  if (collapsePlugins)
    lines.push(
      `  plugins[/"${g.nodes.filter((n) => n.kind === "plugin").length} plugins"/]`,
    );
  const skills = g.nodes.filter((n) => n.kind === "skill").length;
  if (skills > 0 && !g.edges.some((e) => e.kind === "contains"))
    lines.push(`  skills(("${skills} skills"))`);
  for (const n of g.nodes) {
    if (n.kind === "skill" || (collapsePlugins && n.kind === "plugin"))
      continue;
    const [a, b] = shape[n.kind];
    const count = skillCount.get(n.id);
    lines.push(
      `  ${mmId(n.id)}${a}"${mmLabel(n.label)}${count ? ` (${count} skills)` : ""}"${b}`,
    );
  }
  const seen = new Set<string>();
  for (const e of g.edges) {
    if (e.to.startsWith("skill:") || e.from.startsWith("skill:")) continue;
    if (collapsePlugins) {
      const from = e.from.startsWith("plugin:") ? "plugins" : e.from;
      const key = `${from}>${e.to}`;
      if (e.to.startsWith("plugin:") || seen.has(key)) continue;
      seen.add(key);
      lines.push(`  ${mmId(from)} -->|${e.kind}| ${mmId(e.to)}`);
      continue;
    }
    lines.push(
      `  ${mmId(e.from)} ${e.kind === "imports" ? "-.->" : "-->"}|${e.kind}| ${mmId(e.to)}`,
    );
  }
  return `${lines.join("\n")}\n`;
}

if (import.meta.main) {
  const [dir = REPO_ROOT, name = "claude"] = Bun.argv.slice(2);
  const g = buildAny(dir, name);
  const out = process.env.GRAPH_OUT ?? join(REPO_ROOT, "docs", "graph");
  mkdirSync(out, { recursive: true });
  writeFileSync(join(out, `${name}.json`), `${JSON.stringify(g, null, 2)}\n`);
  writeFileSync(join(out, `${name}.mmd`), toMermaid(g));
  const by = (k: NodeKind) => g.nodes.filter((n) => n.kind === k).length;
  console.log(
    `${name}: ${g.nodes.length} nodes (${by("package")} packages, ${by("skill")} skills, ${by("hook")} hooks, ${by("mcp")} mcp, ${by("script")} scripts, ${by("cron")} cron), ${g.edges.length} edges -> ${dirname(join(out, name))}`,
  );
}
