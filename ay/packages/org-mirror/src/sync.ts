/** Orchestrates a sync of one organization: repos, then per repo the git mirror, items, releases, changelogs, manifests, and finally packages. */
import type { Database } from "bun:sqlite";
import { syncChangelogs } from "./changelog.ts";
import { gitDir } from "./db.ts";
import type { GitHub } from "./github.ts";
import { syncGit } from "./git.ts";
import { syncItems } from "./items.ts";
import { syncManifests } from "./manifests.ts";
import { syncPackages } from "./packages.ts";
import type { RegistryOptions } from "./registries.ts";
import { fetchRepo, fetchRepos, storeRepo, syncLabelsAndMilestones } from "./repos.ts";
import { syncReleases } from "./releases.ts";
import { setState } from "./state.ts";

/** `items` is issues and pulls together; the two can also be run alone. */
export const STEPS = ["repos", "git", "items", "issues", "pulls", "releases", "changelog", "packages"] as const;
export type Step = (typeof STEPS)[number];

export interface SyncOptions {
  org: string;
  gh: GitHub;
  db: Database;
  /** Data root holding `<org>/git/<repo>.git`. */
  root: string;
  /** Only these repository names. */
  repos?: string[];
  /** Only the N most recently pushed repositories. */
  top?: number;
  /** Crawl issues and pull requests back to this updatedAt instead of the stored watermark. */
  since?: string;
  /** Deep-sync forks too (metadata of forks is always stored). */
  forks?: boolean;
  steps?: Step[];
  concurrency?: number;
  token?: string;
  registry?: RegistryOptions;
  /** Extra package names as `ecosystem:name`. */
  packages?: string[];
  log?: (line: string) => void;
}

export interface SyncReport {
  repos: number;
  targets: string[];
  errors: { repo: string; step: string; error: string }[];
  packages?: { ok: number; missing: number; foreign: number; error: number };
  graphqlCost: number;
  seconds: number;
}

interface Target { full_name: string; name: string; url: string; default_branch: string | null; is_private: number }

export async function syncOrg(o: SyncOptions): Promise<SyncReport> {
  const started = Date.now();
  const steps = new Set(o.steps ?? STEPS);
  const log = o.log ?? (() => {});
  const run = o.db.query("INSERT INTO sync_runs (started_at, args) VALUES (?, ?)").run(new Date().toISOString(), JSON.stringify({ org: o.org, repos: o.repos, top: o.top, since: o.since, steps: [...steps] }));
  const errors: SyncReport["errors"] = [];
  let repoCount = 0;

  if (steps.has("repos")) {
    if (o.repos?.length) {
      for (const n of o.repos) {
        const r = await fetchRepo(o.gh, o.org, n);
        if (!r) errors.push({ repo: `${o.org}/${n}`, step: "repos", error: "not found" });
        else storeRepo(o.db, o.org, r);
      }
      repoCount = o.repos.length;
    } else {
      const all = await fetchRepos(o.gh, o.org, n => log(`repos: ${n}`));
      for (const r of all) storeRepo(o.db, o.org, r);
      repoCount = all.length;
    }
    log(`repos stored: ${repoCount}`);
  }

  let rows = o.db.query<Target, [string]>("SELECT full_name, name, url, default_branch, is_private FROM repos WHERE org = ?" + (o.forks ? "" : " AND is_fork = 0") + " ORDER BY pushed_at DESC").all(o.org);
  if (o.repos?.length) rows = rows.filter(r => o.repos!.includes(r.name));
  if (o.top) rows = rows.slice(0, o.top);
  const targets = rows.map(r => r.name);

  const guarded = async (t: Target, step: string, fn: () => Promise<unknown>) => {
    try {
      await fn();
    } catch (e) {
      const error = String(e instanceof Error ? e.message : e).slice(0, 400);
      errors.push({ repo: t.full_name, step, error });
      setState(o.db, t.full_name, `error:${step}`, { status: "error", note: error });
      log(`${t.full_name} ${step} failed: ${error}`);
    }
  };

  const deep = async (t: Target) => {
    if (steps.has("repos") && !o.repos?.length) await guarded(t, "repo", async () => {
      const r = await fetchRepo(o.gh, o.org, t.name);
      if (r) storeRepo(o.db, o.org, r);
    });
    const dir = gitDir(o.org, t.name, o.root);
    const gitChain = async () => {
      if (steps.has("git")) await guarded(t, "git", async () => {
        const r = await syncGit(o.db, t.full_name, `${t.url}.git`, dir, t.default_branch, t.is_private ? o.token : undefined);
        log(`${t.full_name} git: ${r.cloned ? "cloned" : "fetched"}, ${r.commits} new commits, ${r.branches} branches, ${r.tags} tags`);
      });
      if (steps.has("changelog")) await guarded(t, "changelog", async () => {
        const r = await syncChangelogs(o.db, t.full_name, dir, t.default_branch);
        const m = await syncManifests(o.db, t.full_name, dir, t.default_branch);
        log(`${t.full_name} changelog: ${r.files} files, ${r.entries} entries; manifests ${m.manifests.length}; tags ${m.tags.join(",") || "-"}`);
      });
    };
    const apiChain = async () => {
      const issues = steps.has("items") || steps.has("issues");
      const pulls = steps.has("items") || steps.has("pulls");
      if (!issues && !pulls && !steps.has("releases")) return;
      await guarded(t, "labels", () => syncLabelsAndMilestones(o.gh, o.db, o.org, t.name));
      if (steps.has("releases")) await guarded(t, "releases", async () => log(`${t.full_name} releases: ${await syncReleases(o.gh, o.db, o.org, t.name)}`));
      if (issues) await guarded(t, "issues", () => syncItems(o.gh, o.db, o.org, t.name, "issue", { since: o.since, log }));
      if (pulls) await guarded(t, "pulls", () => syncItems(o.gh, o.db, o.org, t.name, "pr", { since: o.since, log }));
    };
    await Promise.all([gitChain(), apiChain()]);
  };

  const queue = [...rows];
  await Promise.all(Array.from({ length: Math.min(o.concurrency ?? 3, queue.length) }, async () => {
    for (let t = queue.shift(); t; t = queue.shift()) await deep(t);
  }));

  let packages: SyncReport["packages"];
  if (steps.has("packages")) {
    packages = await syncPackages(o.db, o.org, { ...o.registry, named: o.packages, log, repo: o.repos?.length === 1 ? `${o.org}/${o.repos[0]}` : undefined });
    log(`packages: ${JSON.stringify(packages)}`);
  }
  const seconds = Math.round((Date.now() - started) / 1000);
  o.db.query("UPDATE sync_runs SET finished_at = ?, note = ? WHERE id = ?").run(new Date().toISOString(), errors.length ? `${errors.length} errors` : "ok", run.lastInsertRowid);
  return { repos: repoCount, targets, errors, packages, graphqlCost: o.gh.cost, seconds };
}
