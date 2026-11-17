#!/usr/bin/env bun
// third_party/<name>/README.chromium for every imported upstream, generated from what is recorded elsewhere: the pin in
// tools/upstreams.lock.json, the URL in tools/upstreams.toml, the licences and descriptions in the crates' own manifests.
// Chromium's convention (//docs/adding_to_third_party.md): one metadata file per third-party directory, read by tooling.
//   bun tools/third_party.ts [repo]       (idempotent; run after every import)
import { join } from "node:path";

const repo = Bun.argv[2] ?? ".";
const at = (path: string) => join(repo, path);
const layout = Bun.TOML.parse(await Bun.file(at("tools/layout.toml")).text()) as { third_party: { dir: string } };
const spec = Bun.TOML.parse(await Bun.file(at("tools/upstreams.toml")).text()) as {
  upstream: { name: string; url: string; branch: string; mode?: string; drop?: string[]; local?: string[] }[];
};
const lock = (await Bun.file(at("tools/upstreams.lock.json")).json()) as Record<
  string,
  { sha: string; branch: string; url: string; crates: Record<string, string> }
>;
const root = Bun.TOML.parse(await Bun.file(at("Cargo.toml")).text()) as { workspace: { members: string[] } };
const members = new Set(root.workspace.members);

const dir = layout.third_party.dir;
let written = 0;
for (const u of spec.upstream) {
  const entry = lock[u.name];
  const base = `${dir}/${u.name}`;
  if (
    !entry ||
    !(await Bun.file(at(base))
      .stat()
      .then(
        (s) => s.isDirectory(),
        () => false,
      ))
  )
    continue;

  const crates = await Promise.all(
    Object.values(entry.crates).map(async (path) => {
      const file = Bun.file(at(`${path}/Cargo.toml`));
      if (!(await file.exists())) return null;
      const pkg = (Bun.TOML.parse(await file.text()) as any).package ?? {};
      return {
        path,
        name: pkg.name as string,
        version: pkg.version as string | undefined,
        license: pkg.license as string | undefined,
        description: pkg.description as string | undefined,
        shipped: members.has(path),
      };
    }),
  );
  const found = crates.filter((c): c is NonNullable<typeof c> => c !== null);
  const licenses = [...new Set(found.flatMap((c) => (c.license ? [c.license] : [])))].sort();
  const versions = found.flatMap((c) => (c.version ? [c.version] : []));
  const mostCommon = (xs: string[]) => [...Map.groupBy(xs, (x) => x)].sort((a, b) => b[1].length - a[1].length)[0]?.[0];
  const files = (
    await Array.fromAsync(new Bun.Glob("{LICEN[CS]E*,COPYING*,UNLICENSE*}").scan({ cwd: at(base), dot: false }))
  ).sort();
  const lines = [
    `Name: ${u.name}`,
    `Short Name: ${u.name}`,
    `URL: ${entry.url}`,
    `Version: ${mostCommon(versions) ?? "N/A"}`,
    `Revision: ${entry.sha}`,
    `Update Mechanism: Manual (tools/import.py ${u.name}, history replayed linearly)`,
    `License: ${licenses.join(", ") || "Unknown"}`,
    `License File: ${files.length ? files.join(", ") : "N/A (see the crate manifests)"}`,
    `Security Critical: ${found.some((c) => c.shipped) ? "yes" : "no"}`,
    `Shipped: ${found.some((c) => c.shipped) ? "yes" : "no"}`,
    ``,
    `Description:`,
    found.find((c) => c.description)?.description ?? `Upstream ${u.name}.`,
    ``,
    `Local Modifications:`,
    `The crates (${found.length}) live in ${base}/<crate>, one directory per package; their manifests are rewritten by`,
    `tools/flatten.py (path dependencies, inlined workspace inheritance, one version per crate) and patched by`,
    `tools/fixups.sh and tools/patches/. Everything else of the upstream is in ${base}/ unchanged.${u.drop?.length ? ` Not imported: ${u.drop.join(", ")}.` : ""}`,
    ``,
    ...(u.local ?? []).map((line) => `- ${line}`),
    `Crates:`,
    ...found.map((c) => `  ${c.name} ${c.version ?? ""} ${c.path}${c.shipped ? "" : " (not in the workspace)"}`),
    ``,
  ];
  await Bun.write(at(`${base}/README.chromium`), lines.join("\n"));
  written++;
}
console.log(`README.chromium: ${written} upstreams`);
