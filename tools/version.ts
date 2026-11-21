#!/usr/bin/env bun
// One version for the product: product.toml `version` is the source. Workspace crates follow it through
// `version.workspace = true`; the root `[workspace.package]`, the `fw-*` entries of `[workspace.dependencies]` and the
// files listed in product.toml `[release] version_files` carry a copy, which this tool writes and checks.
//   bun tools/version.ts                 check: list every copy that differs from product.toml, exit 1 if any
//   bun tools/version.ts --expect <v>    check, and also require product.toml to hold <v> (the release tag without its v)
//   bun tools/version.ts set <v>         write <v> into product.toml and every copy
import { basename, join } from "node:path";

const root = join(import.meta.dir, "..");
type Product = { version: string; crate_prefix: string; release?: { version_files?: string[] } };
const product = Bun.TOML.parse(await Bun.file(join(root, "product.toml")).text()) as Product;
const escape = (text: string) => text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

// Each site is a regex whose groups are (before)(version)(after); `all` covers every match instead of the first.
type Site = { file: string; pattern: RegExp; all?: boolean };
const versionField = (file: string): RegExp => {
  const name = basename(file);
  if (name.endsWith(".json")) return /^(  "version": ")([^"]+)(")/m;
  if (name === "pyproject.toml") return /(\[project\][^[]*?\nversion = ")([^"]+)(")/;
  if (name === "plugin.toml") return /(\[plugin\][^[]*?\nversion = ")([^"]+)(")/;
  if (name === "Cargo.toml") return /(\[package\][^[]*?\nversion = ")([^"]+)(")/;
  throw new Error(`${file}: no known version field for this file type`);
};
const sites: Site[] = [
  { file: "product.toml", pattern: /^(version = ")([^"]+)(")/m },
  { file: "Cargo.toml", pattern: /(\[workspace\.package\][^[]*?\nversion = ")([^"]+)(")/ },
  { file: "Cargo.toml", pattern: new RegExp(`^(${escape(product.crate_prefix)}[a-z0-9_-]+ = \\{ version = ")([^"]+)(")`, "gm"), all: true },
  ...(product.release?.version_files ?? []).map((file) => ({ file, pattern: versionField(file) })),
];

async function run(target: string | null): Promise<string[]> {
  const drift: string[] = [];
  const texts = new Map<string, string>();
  for (const site of sites) {
    const path = join(root, site.file);
    if (!(await Bun.file(path).exists())) {
      drift.push(`${site.file}: listed in product.toml [release] version_files but missing`);
      continue;
    }
    const text = texts.get(site.file) ?? (await Bun.file(path).text());
    const every = new RegExp(site.pattern.source, site.pattern.flags.includes("g") ? site.pattern.flags : `${site.pattern.flags}g`);
    const found = [...text.matchAll(every)];
    if (found.length === 0) drift.push(`${site.file}: no version field found`);
    for (const match of site.all ? found : found.slice(0, 1)) {
      if (match[2] !== (target ?? product.version)) drift.push(`${site.file}:${text.slice(0, match.index).split("\n").length}: ${match[2]}`);
    }
    if (target !== null) texts.set(site.file, text.replace(site.pattern, (_, before: string, _old: string, after: string) => `${before}${target}${after}`));
  }
  for (const [file, text] of texts) await Bun.write(join(root, file), text);
  return drift;
}

const args = Bun.argv.slice(2);
if (args[0] === "set") {
  const target = args[1];
  if (!target || !/^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/.test(target)) throw new Error(`usage: bun tools/version.ts set <semver>, got ${target}`);
  const rewritten = await run(target);
  console.log(JSON.stringify({ version: target, rewritten: rewritten.length, files: new Set(sites.map((site) => site.file)).size }));
} else {
  const expected = args[0] === "--expect" ? args[1] : undefined;
  const drift = await run(null);
  if (expected !== undefined && expected !== product.version) drift.unshift(`product.toml: ${product.version}, the release tag asks for ${expected}`);
  if (drift.length) {
    console.error(`version drift against product.toml ${product.version}:\n${drift.join("\n")}`);
    process.exit(1);
  }
  console.log(JSON.stringify({ version: product.version, files: new Set(sites.map((site) => site.file)).size, drift: 0 }));
}
