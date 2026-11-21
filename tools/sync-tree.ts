#!/usr/bin/env bun
// Brings a shared checkout up to date with its upstream without stash, reset or loss, then pushes.
//
//   bun tools/sync-tree.ts [repo-dir ...]        (default: the current directory)
//
// Several agents work in one checkout and some push from elsewhere, so a merge is often blocked by
// local files that already match upstream. For each path that is both dirty here and changed upstream:
// - identical to upstream: set aside, merged, then checked byte for byte;
// - a text file that differs: three-way merged (HEAD, local, upstream) and written back after the merge;
// - a file added upstream that also exists here and is newer than that commit: kept on top of it;
// - a submodule whose local commit is on a remote branch: pointer merged, local checkout restored;
// - other staged entries: unstaged for the merge and staged again, unchanged;
// - anything else: the repository is left untouched and reported.
// On any merge failure every file is put back exactly as it was.

import { $ } from "bun";
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";

type Outcome = { repo: string; ok: boolean; before: string; after: string; held: number; merged3: number; note: string };

const git = (repo: string) => (args: string[]) => $`git -C ${repo} ${args}`.quiet().nothrow();

async function out(repo: string, args: string[]): Promise<string> {
  const r = await git(repo)(args);
  return r.stdout.toString().replace(/\r/g, "").trim();
}

async function counts(repo: string): Promise<string> {
  return (await out(repo, ["rev-list", "--left-right", "--count", "HEAD...@{u}"])).replace(/\s+/, "/");
}

async function blob(repo: string, rev: string, path: string): Promise<string | null> {
  const r = await git(repo)(["rev-parse", "--verify", "--quiet", `${rev}:${path}`]);
  return r.exitCode === 0 ? r.stdout.toString().trim() : null;
}

async function syncRepo(repo: string): Promise<Outcome> {
  const run = git(repo);
  await run(["fetch", "--quiet"]);
  const before = await counts(repo);
  const o: Outcome = { repo, ok: false, before, after: before, held: 0, merged3: 0, note: "" };
  if ((await run(["rev-parse", "--abbrev-ref", "@{u}"])).exitCode !== 0) return { ...o, note: "no upstream branch" };

  const behind = Number(before.split("/")[1] ?? 0);
  if (behind > 0) {
    const changed = new Set((await out(repo, ["diff", "--name-only", "HEAD", "@{u}"])).split("\n").filter(Boolean));
    const dirty = (await out(repo, ["status", "--porcelain", "-uall", "-z"]))
      .split("\0")
      .filter(Boolean)
      .map((l) => l.slice(3));
    const blockers = dirty.filter((p) => changed.has(p));

    const hold = mkdtempSync(join(tmpdir(), "sync-tree-"));
    const threeWay: { path: string; merged: string }[] = [];
    const held: string[] = [];
    const layered: string[] = [];
    const submodules: { path: string; commit: string }[] = [];
    try {
      for (const p of blockers) {
        const abs = join(repo, p);
        const upstream = await blob(repo, "@{u}", p);
        const local = existsSync(abs) ? (await out(repo, ["hash-object", "--", p])) : null;
        if (upstream && local === upstream) {
          held.push(p);
          continue;
        }
        const base = await blob(repo, "HEAD", p);
        const lsTree = await out(repo, ["ls-tree", "@{u}", "--", p]);
        if (lsTree.startsWith("160000")) {
          // Submodule: the merge moves the pointer; the local checkout is restored afterwards if its commit is on a remote branch.
          const sub = join(repo, p);
          const localCommit = await out(sub, ["rev-parse", "HEAD"]);
          const onRemote = await out(sub, ["branch", "-r", "--contains", localCommit]);
          if (!onRemote) return { ...o, note: `submodule ${p} is at ${localCommit}, which no remote branch contains; push it first` };
          const pointer = await blob(repo, "HEAD", p);
          if (pointer) await git(sub)(["checkout", "--quiet", pointer]);
          submodules.push({ path: sub, commit: localCommit });
          continue;
        }
        if (upstream && !base && local) {
          // Added upstream, also present here: keep the local file on top when it is newer than the upstream commit.
          const committed = Number(await out(repo, ["log", "-1", "--format=%ct", "@{u}", "--", p])) * 1000;
          if (statSync(join(repo, p)).mtimeMs > committed) {
            layered.push(p);
            held.push(p);
            continue;
          }
          return { ...o, note: `${p} exists here and upstream; the local copy is older than the upstream commit` };
        }
        if (!upstream || !base || !local) return { ...o, note: `cannot merge ${p}: added or deleted on one side` };
        const tmp = join(hold, "3way", p);
        mkdirSync(dirname(tmp), { recursive: true });
        writeFileSync(`${tmp}.base`, (await run(["show", `HEAD:${p}`])).stdout);
        writeFileSync(`${tmp}.theirs`, (await run(["show", `@{u}:${p}`])).stdout);
        copyFileSync(abs, `${tmp}.ours`);
        const m = await $`git merge-file -p ${`${tmp}.ours`} ${`${tmp}.base`} ${`${tmp}.theirs`}`.quiet().nothrow();
        if (m.exitCode !== 0) return { ...o, note: `conflicting local edit in ${p}; left as is` };
        writeFileSync(`${tmp}.merged`, m.stdout);
        threeWay.push({ path: p, merged: `${tmp}.merged` });
        held.push(p);
      }
      // Set every blocker aside, then make the tree match HEAD for those paths.
      for (const p of held) {
        const abs = join(repo, p);
        const copy = join(hold, "orig", p);
        mkdirSync(dirname(copy), { recursive: true });
        copyFileSync(abs, copy);
        if (await blob(repo, "HEAD", p)) await run(["checkout", "--quiet", "HEAD", "--", p]);
        else rmSync(abs);
      }
      // A merge refuses a non-empty index: record other staged entries, unstage them, and stage them again afterwards.
      const staged: { status: string; path: string; entry: string }[] = [];
      for (const line of (await out(repo, ["diff", "--cached", "--name-status", "--no-renames"])).split("\n").filter(Boolean)) {
        const [status, path] = [line[0], line.slice(line.indexOf("\t") + 1)];
        if (held.includes(path)) continue;
        staged.push({ status, path, entry: status === "D" ? "" : await out(repo, ["ls-files", "-s", "--", path]) });
      }
      if (staged.length) await run(["restore", "--staged", "--", ...staged.map((x) => x.path)]);
      const restage = async () => {
        for (const x of staged) {
          if (x.status === "D") await run(["update-index", "--force-remove", "--", x.path]);
          else {
            const [mode, sha] = x.entry.split(/\s+/);
            await run(["update-index", "--add", "--cacheinfo", `${mode},${sha},${x.path}`]);
          }
        }
      };
      const merge = await run(["merge", "--no-edit", "--quiet", "@{u}"]);
      for (const sm of submodules) await git(sm.path)(["checkout", "--quiet", sm.commit]);
      if (merge.exitCode !== 0) {
        await run(["merge", "--abort"]);
        await restage();
        for (const p of held) copyFileSync(join(hold, "orig", p), join(repo, p));
        return { ...o, note: `merge failed, everything restored: ${merge.stderr.toString().trim().split("\n")[0]}` };
      }
      await restage();
      for (const t of threeWay) copyFileSync(t.merged, join(repo, t.path));
      for (const p of layered) copyFileSync(join(hold, "orig", p), join(repo, p));
      for (const p of held) {
        if (threeWay.some((t) => t.path === p) || layered.includes(p)) continue;
        if (readFileSync(join(repo, p)).compare(readFileSync(join(hold, "orig", p))) !== 0) {
          copyFileSync(join(hold, "orig", p), join(repo, p));
          o.note = `content of ${p} differed after merge; local copy restored`;
        }
      }
      o.held = held.length + submodules.length;
      o.merged3 = threeWay.length;
    } finally {
      rmSync(hold, { recursive: true, force: true });
    }
  }

  if ((await counts(repo)).startsWith("0/") === false && (await counts(repo)).endsWith("/0")) {
    const push = await run(["push", "--quiet"]);
    if (push.exitCode !== 0) return { ...o, after: await counts(repo), note: `push failed: ${push.stderr.toString().trim().split("\n")[0]}` };
  }
  return { ...o, ok: true, after: await counts(repo) };
}

const repos = process.argv.slice(2).map((r) => resolve(r));
for (const repo of repos.length ? repos : [process.cwd()]) {
  const r = await syncRepo(repo);
  console.log(JSON.stringify(r));
}
