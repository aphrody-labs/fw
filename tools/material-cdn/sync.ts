#!/usr/bin/env bun
// material-cdn step 1: fetch the sources the CDN is built from into a work directory.
//   bun tools/material-cdn/sync.ts [--work DIR] [--forks] [--upstream]
// Sources (all aphrody-labs forks of the upstreams, see upstreams.toml):
//   icons  aphrody-labs/material-design-icons  branch master (the untouched upstream mirror; `main` is the trimmed atlas branch)
//          sparse checkout of symbols/web (24px, no GRAD variants), variablefont and LICENSE
//   flex   googlefonts/googlesans-flex         latest release zip (the variable TTF), OFL.txt from the fork
//   code   googlefonts/googlesans-code         latest release zip (two variable TTFs), OFL.txt from the fork
// --forks first fast-forwards every fork from its upstream with `gh repo sync` (needs a token with write access to the forks).
// --upstream reads the upstream repositories directly instead of the forks (CI without a fork-sync token).
import { mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { join, resolve } from "node:path";

const args = process.argv.slice(2);
const opt = (name: string, fallback: string) => {
  const i = args.indexOf(`--${name}`);
  return i >= 0 && args[i + 1] ? args[i + 1] : fallback;
};
const work = resolve(opt("work", ".cache/material-cdn"));
const ORG = "aphrody-labs";
const fromUpstream = args.includes("--upstream");

const FORKS = [
  { id: "icons", repo: "material-design-icons", upstream: "google/material-design-icons", branch: "master" },
  { id: "flex", repo: "googlesans-flex", upstream: "googlefonts/googlesans-flex", branch: "main" },
  { id: "code", repo: "googlesans-code", upstream: "googlefonts/googlesans-code", branch: "main" },
];

async function run(cmd: string[], cwd?: string): Promise<string> {
  const p = Bun.spawn(cmd, { cwd, stdout: "pipe", stderr: "pipe" });
  const [out, err, code] = await Promise.all([new Response(p.stdout).text(), new Response(p.stderr).text(), p.exited]);
  if (code !== 0) throw new Error(`${cmd.join(" ")} (cwd ${cwd ?? "."}) exited ${code}: ${err.trim() || out.trim()}`);
  return out.trim();
}

const ghHeaders: Record<string, string> = { Accept: "application/vnd.github+json", "User-Agent": "fw-material-cdn" };
const ghToken = process.env.GH_TOKEN ?? process.env.GITHUB_TOKEN;
if (ghToken) ghHeaders.Authorization = `Bearer ${ghToken}`;
async function gh<T>(path: string): Promise<T> {
  const r = await fetch(`https://api.github.com/${path}`, { headers: ghHeaders });
  if (!r.ok) throw new Error(`GitHub ${path}: ${r.status} ${await r.text()}`);
  return (await r.json()) as T;
}

async function sha256(path: string): Promise<string> {
  return new Bun.CryptoHasher("sha256").update(new Uint8Array(await Bun.file(path).arrayBuffer())).digest("hex");
}

if (args.includes("--forks")) {
  for (const f of FORKS) {
    const out = await run(["gh", "repo", "sync", `${ORG}/${f.repo}`, "--branch", f.branch]);
    console.log(`fork ${ORG}/${f.repo}@${f.branch}: ${out || "in sync"}`);
  }
}

await mkdir(work, { recursive: true });
const sources: Record<string, unknown> = {};

// icons: sparse partial clone, refreshed in place when it already exists.
const iconsDir = join(work, "material-design-icons");
const iconsRepo = `https://github.com/${fromUpstream ? "google" : ORG}/material-design-icons.git`;
if (!existsSync(join(iconsDir, ".git"))) {
  await run(["git", "clone", "--branch", "master", "--depth", "1", "--filter=blob:none", "--no-checkout", iconsRepo, iconsDir]);
  await run(["git", "sparse-checkout", "init", "--no-cone"], iconsDir);
  await writeFile(
    join(iconsDir, ".git", "info", "sparse-checkout"),
    ["/LICENSE", "/README.md", "/variablefont/", "/symbols/web/*/*/*_24px.svg", "!/symbols/web/*/*/*grad*", ""].join("\n"),
  );
  await run(["git", "checkout", "master"], iconsDir);
} else {
  await run(["git", "fetch", "--depth", "1", "origin", "master"], iconsDir);
  await run(["git", "reset", "--hard", "FETCH_HEAD"], iconsDir);
}
const iconsSha = await run(["git", "rev-parse", "HEAD"], iconsDir);
const iconsDate = await run(["git", "log", "-1", "--format=%cs"], iconsDir);
sources.icons = { repo: "google/material-design-icons", sha: iconsSha, date: iconsDate };
console.log(`icons ${iconsSha.slice(0, 12)} ${iconsDate}`);

// fonts: latest upstream release zip + OFL from the fork.
async function fetchFont(id: "flex" | "code", assetRe: RegExp) {
  const f = FORKS.find((x) => x.id === id)!;
  const rel = await gh<{ tag_name: string; assets: { name: string; browser_download_url: string }[] }>(`repos/${f.upstream}/releases/latest`);
  const asset = rel.assets.find((a) => assetRe.test(a.name));
  if (!asset) throw new Error(`${f.upstream} ${rel.tag_name}: no asset matches ${assetRe}`);
  const dir = join(work, id);
  await rm(dir, { recursive: true, force: true });
  await mkdir(dir, { recursive: true });
  const zip = join(dir, asset.name);
  const res = await fetch(asset.browser_download_url, { headers: { "User-Agent": "fw-material-cdn" } });
  if (!res.ok) throw new Error(`download ${asset.browser_download_url}: ${res.status}`);
  await Bun.write(zip, res);
  await run(["unzip", "-o", "-q", zip, "-d", dir]);
  const ofl = await fetch(`https://raw.githubusercontent.com/${fromUpstream ? f.upstream : `${ORG}/${f.repo}`}/${f.branch}/OFL.txt`);
  if (!ofl.ok) throw new Error(`OFL.txt of ${ORG}/${f.repo}: ${ofl.status}`);
  await writeFile(join(dir, "OFL.txt"), await ofl.text());
  sources[id] = { repo: f.upstream, tag: rel.tag_name, asset: asset.name, zipSha256: await sha256(zip) };
  console.log(`${id} ${rel.tag_name} ${asset.name}`);
}
await fetchFont("flex", /^GoogleSansFlex-v[\d.]+\.zip$/);
await fetchFont("code", /^GoogleSansCode-v[\d.]+\.zip$/);

await writeFile(join(work, "sources.json"), JSON.stringify(sources, null, 2) + "\n");
console.log(`sources -> ${join(work, "sources.json")}`);
