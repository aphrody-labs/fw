#!/usr/bin/env bun
// material-cdn step 1: fetch the pinned sources the CDN is built from into a work directory.
//   bun tools/material-cdn/sync.ts [--work DIR] [--update]
// Sources are read live from the upstream repositories and pinned by commit in tools/material-cdn/lock.json:
//   icons  google/material-design-icons  sparse checkout of symbols/web (24px SVGs, no GRAD variants), variablefont (TTF, WOFF2, codepoints), LICENSE
//   fonts  google/fonts                  one TTF per family under ofl/ (every blob verified against the git blob SHA in the lock) plus its OFL.txt
// Without --update the lock is authoritative: the same commits are fetched, so a build is reproducible.
// --update resolves the latest commit of each upstream branch, rewrites the lock and fetches that.
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { createHash } from "node:crypto";
import { join, resolve } from "node:path";

const args = process.argv.slice(2);
const opt = (name: string, fallback: string) => {
  const i = args.indexOf(`--${name}`);
  return i >= 0 && args[i + 1] ? args[i + 1] : fallback;
};
const work = resolve(opt("work", ".cache/material-cdn"));
const lockPath = join(import.meta.dir, "lock.json");

interface Lock {
  icons: { repo: string; ref: string; sha: string; date: string };
  fonts: { repo: string; ref: string; sha: string; date: string; families: Record<string, { family: string; dir: string; file: string; blob: string; size: number }> };
}

/** Families served by the CDN: the id is the URL slug; `file` is looked up in `ofl/<dir>/` of google/fonts. */
const FAMILIES: Record<string, { family: string; dir: string; match: RegExp }> = {
  "google-sans-flex": { family: "Google Sans Flex", dir: "googlesansflex", match: /^GoogleSansFlex\[[^\]]*\]\.ttf$/ },
  "google-sans-code": { family: "Google Sans Code", dir: "googlesanscode", match: /^GoogleSansCode\[[^\]]*\]\.ttf$/ },
  "google-sans-code-italic": { family: "Google Sans Code", dir: "googlesanscode", match: /^GoogleSansCode-Italic\[[^\]]*\]\.ttf$/ },
  "roboto-flex": { family: "Roboto Flex", dir: "robotoflex", match: /^RobotoFlex\[[^\]]*\]\.ttf$/ },
};

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
const encodePath = (p: string) => p.split("/").map(encodeURIComponent).join("/");

async function resolveRef(repo: string, ref: string): Promise<{ sha: string; date: string }> {
  const c = await gh<{ sha: string; commit: { committer: { date: string } } }>(`repos/${repo}/commits/${ref}`);
  return { sha: c.sha, date: c.commit.committer.date.slice(0, 10) };
}

async function updateLock(prev: Lock | undefined): Promise<Lock> {
  const icons = { repo: "google/material-design-icons", ref: "master", ...(await resolveRef("google/material-design-icons", "master")) };
  const fontsRepo = { repo: "google/fonts", ref: "main" };
  const fontsHead = await resolveRef(fontsRepo.repo, fontsRepo.ref);
  const families: Lock["fonts"]["families"] = {};
  for (const [id, f] of Object.entries(FAMILIES)) {
    const listing = await gh<{ name: string; path: string; sha: string; size: number }[]>(`repos/google/fonts/contents/ofl/${f.dir}?ref=${fontsHead.sha}`);
    const hit = listing.find((e) => f.match.test(e.name));
    if (!hit) throw new Error(`google/fonts@${fontsHead.sha.slice(0, 12)}: no file matching ${f.match} in ofl/${f.dir}`);
    families[id] = { family: f.family, dir: f.dir, file: hit.name, blob: hit.sha, size: hit.size };
  }
  const lock: Lock = { icons, fonts: { ...fontsRepo, ...fontsHead, families } };
  if (JSON.stringify(prev) !== JSON.stringify(lock)) await writeFile(lockPath, JSON.stringify(lock, null, 2) + "\n");
  return lock;
}

const prev: Lock | undefined = existsSync(lockPath) ? JSON.parse(await readFile(lockPath, "utf8")) : undefined;
const lock = args.includes("--update") || !prev ? await updateLock(prev) : prev;
console.log(`lock: icons ${lock.icons.sha.slice(0, 12)} ${lock.icons.date}, fonts ${lock.fonts.sha.slice(0, 12)} ${lock.fonts.date}${prev && JSON.stringify(prev) !== JSON.stringify(lock) ? " (updated)" : ""}`);

await mkdir(work, { recursive: true });

// icons: sparse partial clone fetched at the pinned commit.
const iconsDir = join(work, "material-design-icons");
if (!existsSync(join(iconsDir, ".git"))) {
  await run(["git", "clone", "--depth", "1", "--filter=blob:none", "--no-checkout", `https://github.com/${lock.icons.repo}.git`, iconsDir]);
  await run(["git", "sparse-checkout", "init", "--no-cone"], iconsDir);
  await writeFile(
    join(iconsDir, ".git", "info", "sparse-checkout"),
    ["/LICENSE", "/README.md", "/variablefont/", "/symbols/web/*/*/*_24px.svg", "!/symbols/web/*/*/*grad*", ""].join("\n"),
  );
}
if (!existsSync(join(iconsDir, "LICENSE")) || (await run(["git", "rev-parse", "HEAD"], iconsDir).catch(() => "")) !== lock.icons.sha) {
  await run(["git", "fetch", "--depth", "1", "origin", lock.icons.sha], iconsDir);
  await run(["git", "checkout", "--force", lock.icons.sha], iconsDir);
}
const iconsSha = await run(["git", "rev-parse", "HEAD"], iconsDir);
if (iconsSha !== lock.icons.sha) throw new Error(`icons checkout ${iconsSha} != lock ${lock.icons.sha}`);
console.log(`icons ${iconsSha.slice(0, 12)}`);

// fonts: raw blobs at the pinned commit, each verified against its git blob SHA.
const gitBlobSha = (bytes: Uint8Array) => createHash("sha1").update(`blob ${bytes.length}\0`).update(bytes).digest("hex");
const sha256 = (bytes: Uint8Array) => createHash("sha256").update(bytes).digest("hex");
const fonts: Record<string, { family: string; file: string; path: string; blob: string; sha256: string; size: number }> = {};
async function fetchRaw(path: string): Promise<Uint8Array> {
  const url = `https://raw.githubusercontent.com/${lock.fonts.repo}/${lock.fonts.sha}/${encodePath(path)}`;
  const r = await fetch(url, { headers: { "User-Agent": "fw-material-cdn" } });
  if (!r.ok) throw new Error(`${url}: ${r.status}`);
  return new Uint8Array(await r.arrayBuffer());
}
for (const [id, f] of Object.entries(lock.fonts.families)) {
  const dir = join(work, "fonts", id);
  await mkdir(dir, { recursive: true });
  const target = join(dir, f.file);
  let cached: Uint8Array | undefined = existsSync(target) ? new Uint8Array(await Bun.file(target).arrayBuffer()) : undefined;
  if (!cached || gitBlobSha(cached) !== f.blob) {
    cached = await fetchRaw(`ofl/${f.dir}/${f.file}`);
    if (gitBlobSha(cached) !== f.blob) throw new Error(`${id}: blob sha ${gitBlobSha(cached)} != lock ${f.blob}`);
    await Bun.write(target, cached);
  }
  const bytes: Uint8Array = cached;
  if (!existsSync(join(dir, "OFL.txt"))) await writeFile(join(dir, "OFL.txt"), new TextDecoder().decode(await fetchRaw(`ofl/${f.dir}/OFL.txt`)));
  fonts[id] = { family: f.family, file: f.file, path: `ofl/${f.dir}/${f.file}`, blob: f.blob, sha256: sha256(bytes), size: bytes.length };
  console.log(`font ${id} ${f.file} ${bytes.length} bytes`);
}

const sources = {
  icons: { repo: lock.icons.repo, sha: lock.icons.sha, date: lock.icons.date },
  fonts: { repo: lock.fonts.repo, sha: lock.fonts.sha, date: lock.fonts.date, files: fonts },
};
await writeFile(join(work, "sources.json"), JSON.stringify(sources, null, 2) + "\n");
console.log(`sources -> ${join(work, "sources.json")}`);
