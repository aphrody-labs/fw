#!/usr/bin/env bun
// Linux kernel documentation to reference pages: every Documentation/**/*.rst of a kernel tree is converted with pandoc (a pinned wheel through uvx),
// then written as an MDX page with Bun-docs-style frontmatter (title, description, source path, revision), grouped by area, with one index per area and
// one top-level index. Deterministic: no model is involved; a skill's trigger text per area is added afterwards by a cheap model from these indexes.
// Output is generated data (GPL documentation): it goes to a private, git-ignored directory, never into a tracked repository.
//   bun tools/linux-docs.ts <kernel-tree> <out-dir> [area...]     convert (all areas when none is named); run it where the tree is
import { existsSync, mkdirSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { basename, dirname, join, relative, resolve } from "node:path";

/** Escapes what MDX would parse as JSX or an expression, outside fenced code and inline code. */
export function mdxSafe(markdown: string): string {
  let fenced = false;
  return markdown
    .split("\n")
    .map((line) => {
      if (/^\s*(```|~~~)/.test(line)) {
        fenced = !fenced;
        return line;
      }
      if (fenced) return line;
      return line.split(/(`[^`]*`)/).map((part, i) => (i % 2 ? part : part.replace(/\{/g, "\\{").replace(/\}/g, "\\}").replace(/<(?!\/?(?:https?:|mailto:))/g, "&lt;"))).join("");
    })
    .join("\n");
}

export type Page = { title: string; description: string; source: string; revision: string };

export function frontmatter(p: Page): string {
  const q = (s: string) => JSON.stringify(s.replace(/\s+/g, " ").trim());
  return `---\ntitle: ${q(p.title)}\ndescription: ${q(p.description)}\nsource: ${q(p.source)}\nrevision: ${q(p.revision)}\n---\n\n`;
}

/** Title (first heading) and description (first prose paragraph) of a converted page. */
export function describe(markdown: string, fallback: string): { title: string; description: string; body: string } {
  const lines = markdown.split("\n");
  const h = lines.findIndex((l) => /^#{1,3}\s+\S/.test(l));
  const title = h >= 0 ? lines[h]!.replace(/^#+\s+/, "").replace(/\s*\{[^}]*\}\s*$/, "").trim() : fallback;
  const rest = (h >= 0 ? lines.slice(h + 1) : lines).join("\n");
  const para = rest.split(/\n\s*\n/).map((p) => p.trim()).find((p) => p && !/^(#|```|\||[-*]\s|>|:::)/.test(p));
  const description = (para ?? title).replace(/\[([^\]]+)\]\([^)]*\)/g, "$1").replace(/[`*_]/g, "").replace(/\s+/g, " ").slice(0, 200);
  return { title, description, body: h >= 0 ? [...lines.slice(0, h), ...lines.slice(h + 1)].join("\n") : markdown };
}

const PY = `
import sys, pypandoc
for src, dst in (l.rstrip("\\n").split("\\t") for l in sys.stdin):
    try:
        open(dst, "w", encoding="utf-8").write(pypandoc.convert_file(src, "gfm", format="rst", extra_args=["--wrap=none"]))
    except Exception as e:
        print("FAIL", src, str(e)[:120], file=sys.stderr)
`;

function rstFiles(dir: string, out: string[] = []): string[] {
  for (const e of readdirSync(dir, { withFileTypes: true })) {
    const p = join(dir, e.name);
    if (e.isDirectory()) rstFiles(p, out);
    else if (e.name.endsWith(".rst")) out.push(p);
  }
  return out;
}

export function areaOf(docRoot: string, file: string): string {
  const rel = relative(docRoot, file).replaceAll("\\", "/");
  return rel.includes("/") ? rel.split("/")[0]! : "top-level";
}

if (import.meta.main) {
  const [tree, outArg, ...areas] = process.argv.slice(2);
  if (!tree || !outArg) (console.error("usage: linux-docs.ts <kernel-tree> <out-dir> [area...]"), process.exit(2));
  const docRoot = join(resolve(tree), "Documentation");
  const out = resolve(outArg);
  const revision = Bun.spawnSync(["git", "-C", resolve(tree), "describe", "--tags", "--always"], { stdout: "pipe" }).stdout.toString().trim() || "unknown";
  const files = rstFiles(docRoot).filter((f) => !areas.length || areas.includes(areaOf(docRoot, f)));
  const tmp = join(out, ".md");
  mkdirSync(tmp, { recursive: true });
  const jobs = files.map((f) => [f, join(tmp, relative(docRoot, f).replaceAll(/[\\/]/g, "__").replace(/\.rst$/, ".md"))] as const);
  const proc = Bun.spawnSync(["uvx", "--from", "pypandoc_binary", "python", "-c", PY], { stdin: new Blob([jobs.map(([s, d]) => `${s}\t${d}`).join("\n")]), stdout: "inherit", stderr: "inherit" });
  if (proc.exitCode !== 0) process.exit(proc.exitCode ?? 1);
  const index = new Map<string, { page: string; title: string; description: string }[]>();
  for (const [src, md] of jobs) {
    if (!existsSync(md) || statSync(md).size === 0) continue;
    const rel = relative(docRoot, src).replaceAll("\\", "/");
    const area = areaOf(docRoot, src);
    const text = await Bun.file(md).text();
    const d = describe(text, basename(rel, ".rst"));
    const page = rel.replace(/\.rst$/, ".mdx");
    const dest = join(out, "references", page);
    mkdirSync(dirname(dest), { recursive: true });
    writeFileSync(dest, frontmatter({ title: d.title, description: d.description, source: `Documentation/${rel}`, revision }) + mdxSafe(d.body));
    (index.get(area) ?? index.set(area, []).get(area)!).push({ page, title: d.title, description: d.description });
  }
  for (const [area, pages] of index) {
    const lines = [`---\ntitle: ${JSON.stringify(`Linux ${area} documentation`)}\ndescription: ${JSON.stringify(`${pages.length} pages of Documentation/${area} (${revision})`)}\n---\n`, ...pages.sort((a, b) => a.page.localeCompare(b.page)).map((p) => `- [${p.title.replace(/[\[\]]/g, "")}](./${basename(p.page)}): ${p.description.replace(/[{}<]/g, "")}`), ""];
    const dir = join(out, "references", area === "top-level" ? "." : area);
    mkdirSync(dir, { recursive: true });
    writeFileSync(join(dir, "_index.md"), lines.join("\n"));
  }
  writeFileSync(join(out, "areas.json"), JSON.stringify({ revision, areas: Object.fromEntries([...index].map(([a, p]) => [a, p.length])) }, null, 2));
  console.log(`${files.length} files, ${[...index.values()].reduce((n, p) => n + p.length, 0)} pages, ${index.size} areas, revision ${revision}`);
}
