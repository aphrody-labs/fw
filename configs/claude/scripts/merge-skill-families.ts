#!/usr/bin/env bun
// Merges families of near-related skills of the aphrody plugin into one parent skill per family, without losing content.
//   bun scripts/merge-skill-families.ts [--skills <dir>]... [--dry-run] [--no-backup]
// Source of truth is ~/.gemini/plugins/aphrody/skills (scripts/align-agy.ts mirrors it into packages/aphrody/skills).
// By default both gemini copies (plugins/aphrody and config/plugins/aphrody) are processed.
// Per family: the parent SKILL.md becomes a short router; each old SKILL.md body moves to <parent>/references/<child>.md
// (original frontmatter kept verbatim in an HTML comment, description kept as the first line), every other file of the old
// skill moves to <parent>/references/<child>/, and relative paths inside the moved body are rewritten to stay valid from the
// parent directory. Idempotent: a child already merged (reference file present, directory gone) is skipped.
import {
  cpSync,
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  renameSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { basename, dirname, join, relative } from "node:path";
import { HOME, toSlash } from "./lib.ts";

export const MARKER = "<!-- merge-skill-families:v1 -->";

export interface Family {
  /** Parent skill name. Existing parents keep their old body as references/core.md. */
  parent: string;
  description?: string;
  /** Appended to the existing parent's description instead of replacing it. */
  descriptionExtra?: string;
  title: string;
  intro: string;
  notes?: string[];
  children: string[];
}

export const FAMILIES: Family[] = [
  {
    parent: "replica",
    title: "Replica: clone an app end to end",
    description:
      "Clone an existing app end to end: recon map, stack and schema, design tokens, screen-by-screen build, backend, tests, parity diff, user-review research, rebrand, launch copy and deploy. Use when the user says replicate, clone, rebuild or copy an app, or names a stage (recon, architect, design, build, backend, test, diff, entrepreneur, brand, launch, deploy).",
    intro:
      "Replica is a pipeline. Run the stages in this order and pass each stage's output file to the next: recon, architect, design, build, backend, test, diff, entrepreneur, brand, launch, deploy.",
    notes: [
      "A reference that says `/replica-<stage>` means: open `references/replica-<stage>.md`.",
      "The helper scripts and data files of a stage live under `references/replica-<stage>/`.",
    ],
    children: [
      "replica-recon",
      "replica-architect",
      "replica-design",
      "replica-build",
      "replica-backend",
      "replica-test",
      "replica-diff",
      "replica-entrepreneur",
      "replica-brand",
      "replica-launch",
      "replica-deploy",
    ],
  },
  {
    parent: "docker",
    title: "Docker (except documentation lookup)",
    description:
      "Docker work: Dockerfiles and image size or build speed, Compose stacks, containerizing a project, Docker Agent (agent.yaml, run, deploy), Docker Sandboxes (sbx lifecycle, kits, network and credentials). Read the destructive-guardrails reference BEFORE any docker command that deletes, prunes, resets or cleans state. Use docker-docs for documentation questions.",
    intro:
      "Pick the reference that matches the task and read it in full before acting. Documentation questions about any Docker product go to the separate `docker-docs` skill.",
    notes: [
      'Safety first: before running or recommending any docker command that deletes, wipes, prunes, resets or irreversibly changes state (even for a request like "clean up" or "start fresh"), read `references/docker-destructive-guardrails.md`.',
    ],
    children: [
      "docker-destructive-guardrails",
      "docker-project-foundations",
      "docker-build-strategies",
      "docker-compose-patterns",
      "docker-agent-config",
      "docker-agent-run",
      "docker-agent-deploy",
      "docker-sandboxes-lifecycle",
      "docker-sandboxes-network-credentials",
      "docker-sandboxes-kits",
    ],
  },
  {
    parent: "bun-aphrody",
    title: "Bun with the Aphrody runtime",
    description:
      "Start here for any Bun work: use the Aphrody runtime (aphrody-labs/bun) for everything (bun and bunx instead of node, npm, npx, yarn, pnpm, pip). Use BEFORE any choice of JS/TS tool, script, test, format, typecheck or build. Routes to Bun APIs vs Node/npm, runtime docs, building and testing the runtime, Rust crates, C++/JSC bindings, nginx, video, VPN, yolo, legacy-Node migration (n2b) and the Bun graph.",
    intro:
      "`references/core.md` is the former body of this skill (install, upgrade, runtime rules, memory fiches): read it first, then the reference that matches the task.",
    children: [
      "bun",
      "bun-native",
      "bun-apis",
      "bun-awesome",
      "bun-rust-first",
      "bun-legacy-replace",
      "n2b",
      "bun-deps",
      "bun-runtime",
      "bun-docs",
      "bun-build",
      "bun-tests",
      "bun-crates",
      "cpp-bun",
      "bun-graph",
      "bun-gpui",
      "bun-nginx",
      "bun-video",
      "bun-vpn",
      "bun-yolo",
      "bun-guide",
    ],
  },
  {
    parent: "binary-analysis",
    title: "Binary analysis: Ghidra, triage, malware, firmware, CTF",
    description:
      "Binary reverse engineering with Ghidra and yolo-re: triage an unknown binary, headless Ghidra analysis, live GhidrAssistMCP session, Ghidra GUI control, readable decompilation and renames, malware and firmware analysis, authorized CTF reversing, PyGhidra and local RAG. Use for any binary, firmware image or sample; REA runtime-evidence targets go to reverse-engineer-anything.",
    intro:
      "Start with `references/binary-triage.md` for an unknown binary, then take the reference that matches the next step.",
    children: [
      "binary-triage",
      "ghidra-workflows",
      "ghidra-headless",
      "ghidra-mcp-live",
      "ghidra-gui-computer-use",
      "decompile-cleaner",
      "malware-analyst",
      "firmware-analyst",
      "ctf-solver",
      "python-re",
    ],
  },
  {
    parent: "gpui",
    title: "GPUI: native UI in Rust",
    description:
      "Native UI in Rust with GPUI: core framework (entities, render, elements, Taffy layout, focus, actions, backends), GPUI Kit apps and their design guides, GPUI Fast retained views, gpui::test and Criterion benchmarks. Use before designing or editing native UI, windows, events or GPUI crate internals.",
    intro:
      "`references/core.md` is the former body of this skill (core GPUI): read it first, then the reference for the layer you touch.",
    children: [
      "gpui-brain",
      "gpui-kit",
      "gpui-kit-design-guides",
      "gpui-fast",
      "gpui-test",
      "gpui-bench",
    ],
  },
  {
    parent: "nsh",
    title: "NSH: Native Shell apps",
    descriptionExtra: " Includes the normative NSH design guides.",
    intro:
      "`references/core.md` is the former body of this skill: read it first. Design guides live in `references/nsh-design-guides.md`.",
    children: ["nsh-design-guides"],
  },
  {
    parent: "stripe",
    title: "Stripe and billing",
    description:
      "Stripe and billing: integration decisions (Checkout vs PaymentIntents, Connect, sandboxes), Connect platform setup and connected-account verification requirements, Stripe Apps and extensions, API and SDK version upgrades, docs.stripe.com lookup, Stripe Projects provisioning, paying Stripe businesses, the Stripe directory, and Metronome usage-based billing.",
    intro:
      "Pick the reference that matches the task and read it in full before acting.",
    children: [
      "stripe-best-practices",
      "stripe-docs",
      "connect-recommend",
      "connect-required-verification-information",
      "stripe-apps",
      "upgrade-stripe",
      "stripe-projects",
      "stripe-pay",
      "stripe-directory",
      "metronome",
    ],
  },
  {
    parent: "nextjs",
    title: "Next.js 16.5+",
    description:
      "Next.js 16.5+ work: App Router architecture and proxy.ts, next.config and caching, runtime MCP and devtools, verifying a change in a running dev server, bundle-size audits, adopting Cache Components and Partial Prefetching and optimizing what they surface. Use for any Next.js code, config, performance or debugging task. Cloudflare deployment is the cloudflare plugin's nextjs-on-cloudflare.",
    intro:
      "Architecture and config first (`nextjs-app-router`, `nextjs-config-caching`), then the runtime loop, then the optimization references.",
    children: [
      "nextjs-app-router",
      "nextjs-config-caching",
      "nextjs-devtools-mcp",
      "next-dev-loop",
      "next-bundle-optimizer",
      "next-cache-components-adoption",
      "next-cache-components-optimizer",
      "next-partial-prefetching-adoption",
      "next-partial-prefetching-optimizer",
    ],
  },
  {
    parent: "better-auth",
    title: "Better Auth",
    description:
      "Better Auth in TypeScript apps: scaffold and configure server and client, adapters, sessions, plugins; email/password and verification; organizations and RBAC; two-factor and MFA; security hardening (rate limits, CSRF, trusted origins, secrets, audit). Use when the user mentions Better Auth, auth.ts, login, sign-up, MFA or multi-tenant auth.",
    intro:
      "Scaffold with `create-auth`, then the reference for the feature being configured.",
    children: [
      "create-auth",
      "better-auth-best-practices",
      "email-and-password-best-practices",
      "organization-best-practices",
      "two-factor-authentication-best-practices",
      "better-auth-security-best-practices",
    ],
  },
  {
    parent: "design",
    title: "Design and brand",
    description:
      "Design and brand work: logo, brand identity and voice, corporate identity, banners and social images, HTML or Chart.js slide decks, design tokens and design-system specs, Tailwind and shadcn UI styling, UI/UX and accessibility reasoning, Material 3 and Google design guidance, and the Aphrody design package (Drive, Sheets, Slides, Figma).",
    intro:
      "`references/core.md` is the former body of this skill (logo, CIP, icons, social photos): read it first when the task is one of those, otherwise take the reference that matches.",
    children: [
      "brand",
      "design-system",
      "ui-ux-pro-max",
      "ui-styling",
      "banner-design",
      "slides",
      "material-3",
      "google-design",
      "design-package",
    ],
  },
  {
    parent: "reverse-engineer-anything",
    title: "REA: reverse engineer anything",
    descriptionExtra:
      " Also routes REA release changelogs, REA CI and test-suite audits, and REA tool, CLI and MCP contract design.",
    intro:
      "`references/core.md` is the former body of this skill (REA investigation workflow): read it first. The REA project maintenance references follow.",
    children: ["rea-changelog-update", "rea-ci-tests-audit", "rea-tool-design"],
  },
];

// --- helpers -------------------------------------------------------------------------------------
const FM = /^---\r?\n([\s\S]*?)\r?\n---\r?\n?/;

export function splitSkill(text: string): { fm: string; body: string } {
  const m = FM.exec(text);
  if (!m) throw new Error("no frontmatter");
  return { fm: m[1] ?? "", body: text.slice(m[0].length) };
}

export function parseFm(fm: string): { name: string; description: string } {
  const y = Bun.YAML.parse(fm) as { name?: string; description?: string };
  return {
    name: String(y.name ?? ""),
    description: String(y.description ?? "")
      .replace(/\s+/g, " ")
      .trim(),
  };
}

const esc = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
const KEEP = new Set([
  "SKILL.md",
  "LICENSE",
  "LICENSE.txt",
  "UPSTREAM.md",
  "skill.yaml",
]);

/** Rewrites paths that pointed inside the old skill directory so they stay valid from the parent directory. */
export function rewritePaths(
  body: string,
  child: string,
  entries: { name: string; dir: boolean }[],
): string {
  const cand = entries.filter((e) => !KEEP.has(e.name));
  if (!cand.length) return body;
  const dirs = cand.filter((e) => e.dir).map((e) => esc(e.name));
  const files = cand.filter((e) => !e.dir).map((e) => esc(e.name));
  const alt = [
    ...(dirs.length ? [`(?:${dirs.join("|")})(?=/)`] : []),
    ...(files.length ? [`(?:${files.join("|")})(?![\\w/-])`] : []),
  ].join("|");
  const re = new RegExp(`(?<![\\w./~@-])(?:\\./)?(${alt})`, "g");
  return body.replace(re, (_m, p: string) => `references/${child}/${p}`);
}

function trunc(s: string, n: number): string {
  if (s.length <= n) return s;
  const cut = s.slice(0, n).replace(/\s+\S*$/, "");
  return `${cut}...`;
}

function entriesOf(dir: string): { name: string; dir: boolean }[] {
  return readdirSync(dir, { withFileTypes: true }).map((d) => ({
    name: d.name,
    dir: d.isDirectory(),
  }));
}

function move(src: string, dst: string): void {
  mkdirSync(dirname(dst), { recursive: true });
  try {
    renameSync(src, dst);
  } catch {
    cpSync(src, dst, { recursive: true });
    rmSync(src, { recursive: true, force: true });
  }
}

function walk(dir: string, out: string[] = []): string[] {
  for (const e of readdirSync(dir, { withFileTypes: true })) {
    const p = join(dir, e.name);
    if (e.isDirectory()) walk(p, out);
    else out.push(p);
  }
  return out;
}

/** Markdown links inside moved files that targeted the old SKILL.md now target the moved body. */
function fixSkillLinks(movedDir: string, refFile: string): number {
  let n = 0;
  for (const f of walk(movedDir).filter((p) => p.endsWith(".md"))) {
    const t = readFileSync(f, "utf8");
    const out = t.replace(
      /\]\(([^)#\s]+SKILL\.md)(#[^)\s]*)?\)/g,
      (m, target: string, anchor?: string) => {
        if (/^[a-z]+:/i.test(target)) return m;
        const abs = join(dirname(f), target);
        if (toSlash(abs) !== toSlash(join(movedDir, "SKILL.md"))) return m;
        n++;
        return `](${toSlash(relative(dirname(f), refFile))}${anchor ?? ""})`;
      },
    );
    if (out !== t) writeFileSync(f, out);
  }
  return n;
}

/**
 * Relative imports of moved code that climbed out of the old skill directory (for example ../../../lib/x.ts) now sit two levels
 * deeper (<parent>/references/<child>/), so they get two more `../`.
 */
export function fixEscapingImports(movedDir: string): number {
  let n = 0;
  for (const f of walk(movedDir).filter((p) =>
    /\.(?:[cm]?[jt]s|tsx)$/.test(p),
  )) {
    const depth = toSlash(relative(movedDir, dirname(f)))
      .split("/")
      .filter((s) => s && s !== ".").length;
    const t = readFileSync(f, "utf8");
    const out = t.replace(
      /(\b(?:from|import|require)\s*\(?\s*)(["'])((?:\.\.\/)+)/g,
      (m, pre: string, q: string, ups: string) => {
        if (ups.length / 3 <= depth) return m;
        n++;
        return `${pre}${q}../../${ups}`;
      },
    );
    if (out !== t) writeFileSync(f, out);
  }
  return n;
}

function renderFm(oldFm: string, name: string, description: string): string {
  const lines = oldFm.split(/\r?\n/);
  const kept: string[] = [];
  let skipping = false;
  for (const l of lines) {
    if (/^description:/.test(l)) {
      skipping = true;
      continue;
    }
    if (skipping && (/^\s/.test(l) || l === "")) continue;
    skipping = false;
    if (/^name:/.test(l)) continue;
    kept.push(l);
  }
  return `---\nname: ${name}\ndescription: ${JSON.stringify(description)}\n${kept.length ? `${kept.join("\n")}\n` : ""}---\n`;
}

interface ChildInfo {
  name: string;
  /** File stem under references/ (the skill name, or <name>-skill when the parent already has a file of that name). */
  ref: string;
  description: string;
}

const mergedFrom = (refs: string, c: string): string | undefined =>
  [c, `${c}-skill`].find((s) => {
    const p = join(refs, `${s}.md`);
    return (
      existsSync(p) &&
      readFileSync(p, "utf8").startsWith(`<!-- Merged from skill \`${c}\``)
    );
  });

function router(f: Family, kids: ChildInfo[], hasCore: boolean): string {
  const rows = [
    ...(hasCore
      ? [
          `| [references/core.md](references/core.md) | Former body of this skill (read first) |`,
        ]
      : []),
    ...kids.map(
      (k) =>
        `| [references/${k.ref}.md](references/${k.ref}.md) | ${trunc(k.description, 320).replace(/\|/g, "\\|")} |`,
    ),
  ];
  return [
    `# ${f.title}`,
    "",
    f.intro,
    "",
    "Read the reference that matches the task in full before acting. Paths written inside reference files are relative to this skill's directory (the one holding this SKILL.md). Where a reference names another merged skill, open `references/<name>.md`; its helper files are under `references/<name>/`.",
    ...(f.notes?.length ? ["", ...f.notes.map((n) => `- ${n}`)] : []),
    "",
    "| Reference | Use when |",
    "| --- | --- |",
    ...rows,
    "",
    MARKER,
    "",
  ].join("\n");
}

export interface Result {
  parent: string;
  merged: string[];
  skipped: string[];
  missing: string[];
  rewritten: number;
}

export function mergeFamily(root: string, f: Family, dry = false): Result {
  const res: Result = {
    parent: f.parent,
    merged: [],
    skipped: [],
    missing: [],
    rewritten: 0,
  };
  const pdir = join(root, f.parent);
  const pskill = join(pdir, "SKILL.md");
  const refs = join(pdir, "references");
  const isNew = !existsSync(pdir);
  if (isNew && f.children.every((c) => !existsSync(join(root, c)))) return res;
  let oldFm = `name: ${f.parent}`;
  let oldDesc = "";
  let alreadyRouter = false;
  if (!isNew) {
    const t = readFileSync(pskill, "utf8");
    alreadyRouter = t.includes(MARKER);
    if (!alreadyRouter) {
      const { fm, body } = splitSkill(t);
      oldFm = fm;
      oldDesc = parseFm(fm).description;
      if (existsSync(join(refs, "core.md")))
        throw new Error(`${f.parent}: references/core.md already exists`);
      if (!dry) {
        mkdirSync(refs, { recursive: true });
        writeFileSync(
          join(refs, "core.md"),
          `<!-- Former body of skill \`${f.parent}\`. Original frontmatter:\n${fm}\n-->\n# ${f.parent} (core)\n\n> ${oldDesc}\n\n${body}`,
        );
      }
    } else {
      const core = join(refs, "core.md");
      const m = existsSync(core)
        ? /Original frontmatter:\n([\s\S]*?)\n-->/.exec(
            readFileSync(core, "utf8"),
          )
        : null;
      oldFm = m?.[1] ?? oldFm;
      oldDesc = m ? parseFm(oldFm).description : "";
    }
  }
  const hasCore = !isNew;
  const kids: ChildInfo[] = [];
  for (const c of f.children) {
    const cdir = join(root, c);
    if (!existsSync(cdir)) {
      const done = mergedFrom(refs, c);
      if (done) {
        const m = /Original frontmatter:\n([\s\S]*?)\n-->/.exec(
          readFileSync(join(refs, `${done}.md`), "utf8"),
        );
        kids.push({
          name: c,
          ref: done,
          description: m ? parseFm(m[1] ?? "").description : "",
        });
        res.skipped.push(c);
      } else res.missing.push(c);
      continue;
    }
    const rn =
      existsSync(join(refs, `${c}.md`)) || existsSync(join(refs, c))
        ? `${c}-skill`
        : c;
    if (existsSync(join(refs, `${rn}.md`)) || existsSync(join(refs, rn)))
      throw new Error(`${f.parent}: references/${rn} already exists`);
    const ref = join(refs, `${rn}.md`);
    const { fm, body } = splitSkill(
      readFileSync(join(cdir, "SKILL.md"), "utf8"),
    );
    const info = parseFm(fm);
    kids.push({ name: c, ref: rn, description: info.description });
    const entries = entriesOf(cdir).filter((e) => e.name !== "SKILL.md");
    const newBody = rewritePaths(body, rn, entries);
    if (newBody !== body) res.rewritten++;
    if (dry) {
      res.merged.push(c);
      continue;
    }
    mkdirSync(refs, { recursive: true });
    writeFileSync(
      ref,
      `<!-- Merged from skill \`${c}\` into \`${f.parent}\`. Original frontmatter:\n${fm}\n-->\n# ${c}\n\n> ${info.description}\n\n${newBody}`,
    );
    if (entries.length) {
      const moved = join(refs, rn);
      mkdirSync(moved, { recursive: true });
      for (const e of entries) move(join(cdir, e.name), join(moved, e.name));
      fixSkillLinks(moved, ref);
      fixEscapingImports(moved);
    }
    rmSync(cdir, { recursive: true, force: true });
    res.merged.push(c);
  }
  if (dry) return res;
  mkdirSync(pdir, { recursive: true });
  const description = f.description ?? `${oldDesc}${f.descriptionExtra ?? ""}`;
  writeFileSync(
    pskill,
    renderFm(oldFm, f.parent, description) + router(f, kids, hasCore),
  );
  return res;
}

/** Relative `references/<child>/...` paths named in the parent's markdown that do not exist. */
export function brokenRefs(root: string, parent: string): string[] {
  const pdir = join(root, parent);
  const out: string[] = [];
  const re = /(?<![\w./~@-])references\/[\w.-]+\/[\w./-]*[\w]/g;
  for (const f of [
    join(pdir, "SKILL.md"),
    ...walk(join(pdir, "references")).filter(
      (p) => p.endsWith(".md") && dirname(p) === join(pdir, "references"),
    ),
  ]) {
    for (const m of readFileSync(f, "utf8").matchAll(re)) {
      const p = join(pdir, m[0].replace(/\.+$/, ""));
      if (!existsSync(p)) out.push(`${basename(f)}: ${m[0]}`);
    }
  }
  return out;
}

// --- gemini scripts: keep merged names from being copied or vendored back --------------------------
export function writeMergedList(scriptsDir: string): void {
  const map: Record<string, string> = {};
  for (const f of FAMILIES) for (const c of f.children) map[c] = f.parent;
  writeFileSync(
    join(scriptsDir, "merged-skills.json"),
    `${JSON.stringify(map, null, 2)}\n`,
  );
  const vs = join(scriptsDir, "vendor-skills.ts");
  if (!existsSync(vs)) return;
  const t = readFileSync(vs, "utf8");
  if (t.includes("merged-skills.json")) return;
  const head = `import merged from "./merged-skills.json" with { type: "json" };`;
  const decl = "export const SUPERSEDED: Record<string, string> = {\n";
  if (!t.includes(decl))
    throw new Error("vendor-skills.ts: SUPERSEDED declaration not found");
  const spread = `  ...Object.fromEntries(Object.entries(merged).map(([c, p]) => [c, \`merged into the \${p} skill (references/\${c}.md)\`])),\n`;
  writeFileSync(vs, insertImport(t, head).replace(decl, decl + spread));
}

function insertImport(t: string, line: string): string {
  const lines = t.split("\n");
  const last = lines.reduce((acc, l, i) => (/^import\b/.test(l) ? i : acc), -1);
  lines.splice(last + 1, 0, line);
  return lines.join("\n");
}

// --- cli -------------------------------------------------------------------------------------------
async function backup(roots: string[]): Promise<string> {
  const stamp = new Date()
    .toISOString()
    .replace(/[-:]/g, "")
    .replace(/\.\d+Z$/, "Z");
  const dir = join(HOME, ".claude", "backups", `merge-skills-${stamp}`);
  mkdirSync(dir, { recursive: true });
  for (const [i, r] of roots.entries()) {
    const out = join(dir, `skills-${i}-${basename(dirname(r))}.tar.gz`);
    const p = Bun.spawn(["tar", "-czf", out, "-C", dirname(r), basename(r)], {
      stdout: "inherit",
      stderr: "inherit",
    });
    if ((await p.exited) !== 0) throw new Error(`tar failed for ${r}`);
  }
  return dir;
}

if (import.meta.main) {
  const argv = Bun.argv.slice(2);
  const dry = argv.includes("--dry-run");
  const roots: string[] = [];
  for (let i = 0; i < argv.length; i++)
    if (argv[i] === "--skills") roots.push(argv[++i] ?? "");
  if (!roots.length)
    roots.push(
      join(HOME, ".gemini", "plugins", "aphrody", "skills"),
      join(HOME, ".gemini", "config", "plugins", "aphrody", "skills"),
    );
  const live = roots.filter((r) => existsSync(r) && statSync(r).isDirectory());
  if (!dry && !argv.includes("--no-backup"))
    console.log("backup:", await backup(live));
  for (const r of live) {
    for (const f of FAMILIES) {
      const res = mergeFamily(r, f, dry);
      const bad = dry ? [] : brokenRefs(r, f.parent);
      console.log(
        JSON.stringify({
          root: toSlash(r).replace(HOME, "~"),
          parent: res.parent,
          merged: res.merged.length,
          skipped: res.skipped.length,
          missing: res.missing,
          rewrittenBodies: res.rewritten,
          brokenRefs: bad,
        }),
      );
    }
  }
  if (!dry && !argv.includes("--skills"))
    writeMergedList(join(HOME, ".gemini", "scripts"));
  console.log("done");
}
