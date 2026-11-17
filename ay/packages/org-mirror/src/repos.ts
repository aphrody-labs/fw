/** Repositories of an owner (organization or user): metadata, languages, topics, labels and milestones, through GraphQL. */
import type { Database } from "bun:sqlite";
import { GitHub, paginate } from "./github.ts";
import { tx } from "./db.ts";
import { TAG_SHARE, languageTag } from "./manifests.ts";

export interface RepoNode {
  id: string;
  name: string;
  nameWithOwner: string;
  description: string | null;
  homepageUrl: string | null;
  url: string;
  isFork: boolean;
  isArchived: boolean;
  isPrivate: boolean;
  isTemplate: boolean;
  stargazerCount: number;
  forkCount: number;
  diskUsage: number | null;
  createdAt: string;
  updatedAt: string;
  pushedAt: string | null;
  defaultBranchRef: { name: string; target: { history?: { totalCount: number } } | null } | null;
  licenseInfo: { spdxId: string | null; name: string } | null;
  primaryLanguage: { name: string } | null;
  watchers: { totalCount: number };
  openIssues: { totalCount: number };
  openPrs: { totalCount: number };
  allIssues: { totalCount: number };
  allPrs: { totalCount: number };
  releases: { totalCount: number };
  labels: { totalCount: number };
  milestones: { totalCount: number };
  languages: { totalSize: number; edges: { size: number; node: { name: string } }[] };
  repositoryTopics: { nodes: { topic: { name: string } }[] };
}

const REPO_FIELDS = /* graphql */ `
  id name nameWithOwner description homepageUrl url isFork isArchived isPrivate isTemplate stargazerCount forkCount diskUsage createdAt updatedAt pushedAt
  defaultBranchRef { name }
  licenseInfo { spdxId name } primaryLanguage { name } watchers { totalCount }
  openIssues: issues(states: OPEN) { totalCount } openPrs: pullRequests(states: OPEN) { totalCount }
  allIssues: issues { totalCount } allPrs: pullRequests { totalCount }
  releases { totalCount } labels { totalCount } milestones { totalCount }
  languages(first: 30, orderBy: { field: SIZE, direction: DESC }) { totalSize edges { size node { name } } }
  repositoryTopics(first: 30) { nodes { topic { name } } }`;

/** The commit count of the default branch is expensive for GitHub, so only the single-repository query asks for it. */
const HISTORY = "defaultBranchRef { target { ... on Commit { history { totalCount } } } }";

const LIST = /* graphql */ `query($owner: String!, $first: Int!, $after: String) {
  repositoryOwner(login: $owner) { repositories(first: $first, after: $after, orderBy: { field: PUSHED_AT, direction: DESC }) {
    pageInfo { hasNextPage endCursor } nodes { ${REPO_FIELDS} } } }
  rateLimit { cost remaining }
}`;

const ONE = /* graphql */ `query($owner: String!, $name: String!) { repository(owner: $owner, name: $name) { ${REPO_FIELDS} ${HISTORY} } rateLimit { cost remaining } }`;

export async function fetchRepos(gh: GitHub, owner: string, onPage?: (n: number) => void): Promise<RepoNode[]> {
  const out: RepoNode[] = [];
  await paginate<RepoNode>(
    25,
    async (after, first) => {
      const d = await gh.graphql<{ repositoryOwner: { repositories: { pageInfo: { hasNextPage: boolean; endCursor: string }; nodes: RepoNode[] } } | null }>(LIST, { owner, first, after });
      if (!d.repositoryOwner) throw new Error(`GitHub owner not found: ${owner}`);
      const r = d.repositoryOwner.repositories;
      return { items: r.nodes.filter(Boolean), next: r.pageInfo.hasNextPage ? r.pageInfo.endCursor : null };
    },
    items => {
      out.push(...items);
      onPage?.(out.length);
    },
  );
  return out;
}

export async function fetchRepo(gh: GitHub, owner: string, name: string): Promise<RepoNode | undefined> {
  const d = await gh.graphql<{ repository: RepoNode | null }>(ONE, { owner, name });
  return d.repository ?? undefined;
}

const now = () => new Date().toISOString();

/** Upsert one repository and replace its language and topic rows. Counts of the git side are kept. */
export function storeRepo(db: Database, org: string, r: RepoNode): void {
  const full = r.nameWithOwner;
  const total = r.languages.totalSize || 1;
  tx(db, () => {
    db.query(
      `INSERT INTO repos (full_name, org, name, node_id, description, homepage, url, default_branch, is_fork, is_archived, is_private, is_template, stars, forks, watchers,
         open_issues, open_prs, issues_total, prs_total, releases_total, labels_total, milestones_total, default_commits_github, disk_kb, license, primary_language,
         created_at, updated_at, pushed_at, synced_at)
       VALUES ($full, $org, $name, $id, $desc, $home, $url, $branch, $fork, $arch, $priv, $tpl, $stars, $forks, $watchers, $oi, $op, $it, $pt, $rt, $lt, $mt, $dc, $disk, $lic, $lang, $created, $updated, $pushed, $at)
       ON CONFLICT(full_name) DO UPDATE SET node_id=$id, description=$desc, homepage=$home, url=$url, default_branch=$branch, is_fork=$fork, is_archived=$arch, is_private=$priv, is_template=$tpl,
         stars=$stars, forks=$forks, watchers=$watchers, open_issues=$oi, open_prs=$op, issues_total=$it, prs_total=$pt, releases_total=$rt, labels_total=$lt, milestones_total=$mt,
         default_commits_github=coalesce($dc, default_commits_github), disk_kb=$disk, license=$lic, primary_language=$lang, created_at=$created, updated_at=$updated, pushed_at=$pushed, synced_at=$at`,
    ).run({
      $full: full, $org: org, $name: r.name, $id: r.id, $desc: r.description, $home: r.homepageUrl || null, $url: r.url, $branch: r.defaultBranchRef?.name ?? null,
      $fork: +r.isFork, $arch: +r.isArchived, $priv: +r.isPrivate, $tpl: +r.isTemplate, $stars: r.stargazerCount, $forks: r.forkCount, $watchers: r.watchers.totalCount,
      $oi: r.openIssues.totalCount, $op: r.openPrs.totalCount, $it: r.allIssues.totalCount, $pt: r.allPrs.totalCount, $rt: r.releases.totalCount,
      $lt: r.labels.totalCount, $mt: r.milestones.totalCount, $dc: r.defaultBranchRef?.target?.history?.totalCount ?? null, $disk: r.diskUsage,
      $lic: r.licenseInfo?.spdxId ?? r.licenseInfo?.name ?? null, $lang: r.primaryLanguage?.name ?? null,
      $created: r.createdAt, $updated: r.updatedAt, $pushed: r.pushedAt, $at: now(),
    });
    db.query("DELETE FROM repo_languages WHERE repo = ?").run(full);
    const lang = db.query("INSERT INTO repo_languages (repo, language, bytes, share) VALUES (?, ?, ?, ?)");
    for (const e of r.languages.edges) lang.run(full, e.node.name, e.size, e.size / total);
    db.query("DELETE FROM repo_topics WHERE repo = ?").run(full);
    const topic = db.query("INSERT OR IGNORE INTO repo_topics (repo, topic) VALUES (?, ?)");
    for (const t of r.repositoryTopics.nodes) topic.run(full, t.topic.name);
    // Language tags for every repository; the manifest step later refines them for the deep-synced ones.
    db.query("DELETE FROM repo_tags WHERE repo = ? AND via = 'language'").run(full);
    const tag = db.query("INSERT OR IGNORE INTO repo_tags (repo, tag, via, share) VALUES (?, ?, 'language', ?)");
    for (const e of r.languages.edges) {
      const t = languageTag(e.node.name);
      if (t && e.size / total >= TAG_SHARE) tag.run(full, t, e.size / total);
    }
  });
}

interface LabelNode { name: string; color: string; description: string | null }
interface MilestoneNode { number: number; title: string; state: string; description: string | null; dueOn: string | null; createdAt: string; closedAt: string | null; issues: { totalCount: number }; closed: { totalCount: number } }

/** Labels and milestones of a repository (small, replaced as a whole). */
export async function syncLabelsAndMilestones(gh: GitHub, db: Database, owner: string, name: string): Promise<void> {
  const full = `${owner}/${name}`;
  const labels: LabelNode[] = [];
  await paginate<LabelNode>(
    100,
    async (after, first) => {
      const d = await gh.graphql<{ repository: { labels: { pageInfo: { hasNextPage: boolean; endCursor: string }; nodes: LabelNode[] } } }>(
        `query($o: String!, $n: String!, $first: Int!, $after: String) { repository(owner: $o, name: $n) { labels(first: $first, after: $after) { pageInfo { hasNextPage endCursor } nodes { name color description } } } rateLimit { cost } }`,
        { o: owner, n: name, first, after },
      );
      const l = d.repository.labels;
      return { items: l.nodes, next: l.pageInfo.hasNextPage ? l.pageInfo.endCursor : null };
    },
    items => void labels.push(...items),
  );
  const milestones: MilestoneNode[] = [];
  await paginate<MilestoneNode>(
    100,
    async (after, first) => {
      const d = await gh.graphql<{ repository: { milestones: { pageInfo: { hasNextPage: boolean; endCursor: string }; nodes: MilestoneNode[] } } }>(
        `query($o: String!, $n: String!, $first: Int!, $after: String) { repository(owner: $o, name: $n) { milestones(first: $first, after: $after, states: [OPEN, CLOSED]) { pageInfo { hasNextPage endCursor } nodes { number title state description dueOn createdAt closedAt issues { totalCount } closed: issues(states: CLOSED) { totalCount } } } } rateLimit { cost } }`,
        { o: owner, n: name, first, after },
      );
      const m = d.repository.milestones;
      return { items: m.nodes, next: m.pageInfo.hasNextPage ? m.pageInfo.endCursor : null };
    },
    items => void milestones.push(...items),
  );
  tx(db, () => {
    db.query("DELETE FROM labels WHERE repo = ?").run(full);
    const l = db.query("INSERT OR REPLACE INTO labels (repo, name, color, description) VALUES (?, ?, ?, ?)");
    for (const x of labels) l.run(full, x.name, x.color, x.description);
    db.query("DELETE FROM milestones WHERE repo = ?").run(full);
    const m = db.query("INSERT OR REPLACE INTO milestones (repo, number, title, state, description, due_on, created_at, closed_at, open_issues, closed_issues) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)");
    for (const x of milestones) m.run(full, x.number, x.title, x.state, x.description, x.dueOn, x.createdAt, x.closedAt, x.issues.totalCount - x.closed.totalCount, x.closed.totalCount);
  });
}
