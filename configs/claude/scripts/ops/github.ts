#!/usr/bin/env bun
// GitHub hygiene for the authenticated user and the organisations it administers.
//   bun scripts/ops/github.ts audit        report only (default)
//   bun scripts/ops/github.ts fix          apply every fix the audit finds
// Fixes: fill missing descriptions (forks from their parent, sources from the README's first
// paragraph), delete archived repositories whose content survives elsewhere (canonical copy in
// another owner, or a fork with no commits of its own), regenerate profile READMEs.
// Requires gh logged in with repo, delete_repo and admin:org scopes.

interface Repo {
  name: string;
  full_name: string;
  owner: { login: string };
  description: string | null;
  fork: boolean;
  archived: boolean;
  private: boolean;
  default_branch: string;
  parent?: {
    full_name: string;
    default_branch: string;
    description: string | null;
  };
}

if (process.argv.includes("--help")) {
  console.log(
    "usage: bun scripts/ops/github.ts [audit|fix]  (audit is the default and read-only)",
  );
  process.exit(0);
}

const apply = process.argv[2] === "fix";

async function api<T>(
  path: string,
  init?: { method?: string; body?: unknown },
): Promise<T | null> {
  const args = [
    "api",
    path,
    ...(init?.method ? ["-X", init.method] : []),
    ...(init?.body ? ["--input", "-"] : []),
  ];
  const p = Bun.spawn(["gh", ...args], {
    stdin: init?.body ? new Blob([JSON.stringify(init.body)]) : "ignore",
    stdout: "pipe",
    stderr: "pipe",
  });
  const [out, code] = await Promise.all([
    new Response(p.stdout).text(),
    p.exited,
  ]);
  if (code !== 0) return null;
  return out.trim() ? (JSON.parse(out) as T) : ({} as T);
}

async function list(owner: string, isOrg: boolean): Promise<Repo[]> {
  const all: Repo[] = [];
  for (let page = 1; ; page++) {
    const batch = await api<Repo[]>(
      `${isOrg ? `orgs/${owner}/repos?type=all` : `user/repos?affiliation=owner`}&per_page=100&page=${page}`,
    );
    if (!batch?.length) break;
    all.push(...batch);
    if (batch.length < 100) break;
  }
  return all;
}

/** Bounded parallel map: keeps the GitHub API under its secondary rate limit. */
async function pool<T, R>(
  items: T[],
  size: number,
  fn: (x: T) => Promise<R>,
): Promise<R[]> {
  const out = Array.from<R>({ length: items.length });
  let i = 0;
  await Promise.all(
    Array.from({ length: Math.min(size, items.length) }, async () => {
      while (i < items.length) {
        const k = i++;
        const item = items[k];
        if (item !== undefined) out[k] = await fn(item);
      }
    }),
  );
  return out;
}

const ahead = async (
  base: string,
  baseBranch: string,
  head: string,
  headBranch: string,
) =>
  (
    await api<{ ahead_by: number }>(
      `repos/${base}/compare/${encodeURIComponent(baseBranch)}...${head.split("/")[0]}:${encodeURIComponent(headBranch)}`,
    )
  )?.ahead_by ?? -1;

async function readmeSummary(r: Repo): Promise<string | null> {
  const f = await api<{ content: string }>(`repos/${r.full_name}/readme`);
  if (!f) return null;
  const text = Buffer.from(f.content, "base64").toString("utf8");
  const para = text
    .replace(/<!--[\s\S]*?-->/g, "")
    .replace(/^---\n[\s\S]*?\n---\n/, "")
    .split(/\n\s*\n/)
    .map((s) => s.trim())
    .map((s) => s.replace(/^>\s*/gm, "").replace(/^"|"$/g, ""))
    .find(
      (s) =>
        s &&
        !s.startsWith("#") &&
        !s.startsWith("---") &&
        !s.startsWith("<") &&
        !s.startsWith("[!") &&
        !s.startsWith("|") &&
        !s.startsWith("```") &&
        !/^!\[/.test(s),
    );
  return para
    ? para.replace(/\s+/g, " ").replace(/[*_`]/g, "").slice(0, 300)
    : null;
}

const self = await api<{ login: string }>("user");
if (!self) throw new Error("gh api user failed: run gh auth login");
const me = self.login;
const orgs = (
  (await api<{ role: string; organization: { login: string } }[]>(
    "user/memberships/orgs?state=active&per_page=100",
  )) ?? []
)
  .filter((m) => m.role === "admin")
  .map((m) => m.organization.login);
const owners: { login: string; isOrg: boolean }[] = [
  { login: me, isOrg: false },
  ...orgs.map((login) => ({ login, isOrg: true })),
];
const repos = [
  ...new Map(
    (await Promise.all(owners.map((o) => list(o.login, o.isOrg))))
      .flat()
      .map((r) => [r.full_name, r]),
  ).values(),
];
const byName = new Map<string, Repo[]>();
for (const r of repos)
  byName.set(r.name.toLowerCase(), [
    ...(byName.get(r.name.toLowerCase()) ?? []),
    r,
  ]);
console.log(
  `owners: ${owners.map((o) => o.login).join(", ")}; repositories: ${repos.length}`,
);

// Archived repositories: delete when their content survives elsewhere.
const archived = repos.filter((r) => r.archived);
const verdicts = await pool(archived, 8, async (r) => {
  const full = (await api<Repo>(`repos/${r.full_name}`)) ?? r;
  const twin = (byName.get(r.name.toLowerCase()) ?? []).find(
    (x) => x.full_name !== r.full_name && !x.archived,
  );
  if (twin) {
    const a = await ahead(
      twin.full_name,
      twin.default_branch,
      r.full_name,
      r.default_branch,
    );
    if (a === 0)
      return { r, del: true, why: `canonical ${twin.full_name} contains it` };
  }
  if (full.parent) {
    const a = await ahead(
      full.parent.full_name,
      full.parent.default_branch,
      r.full_name,
      r.default_branch,
    );
    if (a === 0)
      return {
        r,
        del: true,
        why: `fork with no own commits on ${r.default_branch} (parent ${full.parent.full_name})`,
      };
    return {
      r,
      del: false,
      why: `fork ${a} commit(s) ahead of ${full.parent.full_name}`,
    };
  }
  return {
    r,
    del: false,
    why: twin
      ? `differs from ${twin.full_name}`
      : "source repository, no copy elsewhere",
  };
});
for (const v of verdicts) {
  console.log(`${v.del ? "delete" : "keep  "} ${v.r.full_name}: ${v.why}`);
  if (apply && v.del) {
    const ok = await api(`repos/${v.r.full_name}`, { method: "DELETE" });
    if (!ok) console.log(`  FAILED delete ${v.r.full_name}`);
  }
}

// Missing descriptions on live repositories.
const bare = repos.filter((r) => !r.archived && !(r.description ?? "").trim());
await pool(bare, 8, async (r) => {
  const full = (await api<Repo>(`repos/${r.full_name}`)) ?? r;
  const desc = full.parent
    ? `Fork of ${full.parent.full_name}${full.parent.description ? `: ${full.parent.description}` : ""}`
    : await readmeSummary(r);
  console.log(`describe ${r.full_name}: ${desc ?? "(no README, left empty)"}`);
  if (apply && desc)
    await api(`repos/${r.full_name}`, {
      method: "PATCH",
      body: { description: desc.slice(0, 350) },
    });
});

// Profile READMEs regenerate from their own workflow.
if (apply) {
  for (const repo of [`${me}/${me}`, ...orgs.map((o) => `${o}/.github`)]) {
    const ok = await api(
      `repos/${repo}/actions/workflows/readme.yml/dispatches`,
      { method: "POST", body: { ref: "main" } },
    );
    console.log(`${ok ? "refresh" : "skip   "} profile ${repo}`);
  }
}

export {};
