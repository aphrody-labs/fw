#!/usr/bin/env bun
// Keeps every non-archived fork of the organisation on its upstream's latest commit (GitHub merge-upstream: a fast-forward
// when the fork has no commits of its own, a merge commit otherwise). A fork whose merge conflicts is listed in one issue
// of this repository, updated on every run, so a human or an agent resolves it the way aphrody resolves Zed.
//   GH_TOKEN=<token with contents:write on the org> bun tools/forks-follow.ts [--org aphrody-labs] [--only a,b] [--dry-run]
const args = Bun.argv.slice(2);
const flag = (name: string) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : undefined; };
const org = flag("--org") ?? Bun.env.GITHUB_REPOSITORY_OWNER ?? "aphrody-labs";
const only = flag("--only")?.split(",");
const dryRun = args.includes("--dry-run");
const token = Bun.env.GH_TOKEN ?? Bun.env.GITHUB_TOKEN;
if (!token) throw new Error("GH_TOKEN is not set");

const api = async (path: string, init: RequestInit = {}) => {
  const response = await fetch(`https://api.github.com/${path}`, {
    ...init,
    headers: { authorization: `Bearer ${token}`, accept: "application/vnd.github+json", "x-github-api-version": "2022-11-28", "user-agent": "fw-forks-follow", ...init.headers },
  });
  return { status: response.status, body: response.status === 204 ? null : ((await response.json().catch(() => null)) as any) };
};

type Fork = { name: string; default_branch: string; archived: boolean };
const forks: Fork[] = [];
for (let page = 1; ; page++) {
  const { status, body } = await api(`orgs/${org}/repos?type=forks&per_page=100&page=${page}`);
  if (status !== 200) throw new Error(`list forks: ${status} ${JSON.stringify(body)}`);
  forks.push(...(body as Fork[]));
  if (body.length < 100) break;
}
const targets = forks.filter((f) => !f.archived && (!only || only.includes(f.name)));

type Result = { name: string; outcome: "updated" | "current" | "conflict" | "error"; detail?: string };
const results: Result[] = [];
const queue = [...targets];
await Promise.all(
  Array.from({ length: 8 }, async () => {
    for (let fork = queue.shift(); fork; fork = queue.shift()) {
      if (dryRun) { results.push({ name: fork.name, outcome: "current", detail: "dry run" }); continue; }
      const { status, body } = await api(`repos/${org}/${fork.name}/merge-upstream`, { method: "POST", body: JSON.stringify({ branch: fork.default_branch }) });
      if (status === 200) results.push({ name: fork.name, outcome: body?.merge_type === "none" ? "current" : "updated", detail: body?.merge_type });
      else if (status === 409) results.push({ name: fork.name, outcome: "conflict", detail: body?.message });
      else results.push({ name: fork.name, outcome: "error", detail: `${status} ${body?.message ?? ""}`.trim() });
    }
  }),
);

const count = (outcome: Result["outcome"]) => results.filter((r) => r.outcome === outcome);
const summary = { org, forks: targets.length, updated: count("updated").length, current: count("current").length, conflict: count("conflict").map((r) => r.name), error: count("error").map((r) => `${r.name}: ${r.detail}`) };
console.log(JSON.stringify(summary, null, 1));

// One issue lists what needs a human merge; it is closed when nothing does.
const repo = Bun.env.GITHUB_REPOSITORY;
if (repo && !dryRun) {
  const title = "Forks that cannot follow their upstream automatically";
  const { body: open } = await api(`repos/${repo}/issues?state=open&labels=forks-follow&per_page=1`);
  const existing = (open as any[])?.[0];
  const stuck = [...count("conflict"), ...count("error")];
  const text = [`Run ${Bun.env.GITHUB_SERVER_URL ?? "https://github.com"}/${repo}/actions/runs/${Bun.env.GITHUB_RUN_ID ?? ""}, ${new Date().toISOString()}.`, "", "| Fork | Outcome | Detail |", "|---|---|---|", ...stuck.map((r) => `| [${r.name}](https://github.com/${org}/${r.name}) | ${r.outcome} | ${(r.detail ?? "").replace(/\|/g, "/")} |`)].join("\n");
  if (stuck.length && existing) await api(`repos/${repo}/issues/${existing.number}`, { method: "PATCH", body: JSON.stringify({ body: text }) });
  else if (stuck.length) await api(`repos/${repo}/issues`, { method: "POST", body: JSON.stringify({ title, body: text, labels: ["forks-follow"] }) });
  else if (existing) await api(`repos/${repo}/issues/${existing.number}`, { method: "PATCH", body: JSON.stringify({ state: "closed" }) });
}
if (summary.error.length) process.exit(1);
