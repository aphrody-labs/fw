/** Issues and pull requests with comments, reviews, labels and timeline basics, crawled newest-update-first through GraphQL cursors. */
import type { Database } from "bun:sqlite";
import { GitHub, GitHubError, paginate } from "./github.ts";
import { tx } from "./db.ts";
import { getState, setState } from "./state.ts";

export type Kind = "issue" | "pr";

interface Login { login: string }
export interface CommentNode { id: string; author: Login | null; body: string; createdAt: string; updatedAt: string; url: string }
export interface ReviewNode { id: string; author: Login | null; state: string; body: string; submittedAt: string | null }
export interface EventNode { __typename: string; id: string; createdAt: string; actor?: Login | null; [k: string]: unknown }
interface Conn<T> { totalCount?: number; pageInfo: { hasNextPage: boolean; endCursor: string | null }; nodes: T[] }

export interface ItemNode {
  id: string;
  number: number;
  title: string;
  body: string;
  state: string;
  stateReason?: string | null;
  createdAt: string;
  updatedAt: string;
  closedAt: string | null;
  locked: boolean;
  url: string;
  author: Login | null;
  milestone: { number: number } | null;
  assignees: { nodes: Login[] };
  reactions: { totalCount: number };
  labels: Conn<{ name: string }>;
  comments: Conn<CommentNode> & { totalCount: number };
  timelineItems: Conn<EventNode>;
  // pull requests
  isDraft?: boolean;
  merged?: boolean;
  mergedAt?: string | null;
  mergedBy?: Login | null;
  baseRefName?: string;
  headRefName?: string;
  headRefOid?: string;
  additions?: number;
  deletions?: number;
  changedFiles?: number;
  reviewDecision?: string | null;
  commits?: { totalCount: number };
  reviews?: Conn<ReviewNode> & { totalCount: number };
}

const ACTOR = "id createdAt actor { login }";
/** Timeline item types and fragments: the Issue and PullRequest unions differ, so each kind gets its own. */
const ISSUE_TYPES = ["CLOSED_EVENT", "REOPENED_EVENT", "LABELED_EVENT", "UNLABELED_EVENT", "ASSIGNED_EVENT", "UNASSIGNED_EVENT", "MILESTONED_EVENT", "DEMILESTONED_EVENT", "RENAMED_TITLE_EVENT", "CROSS_REFERENCED_EVENT", "LOCKED_EVENT", "UNLOCKED_EVENT"];
const PR_TYPES = [...ISSUE_TYPES, "MERGED_EVENT", "CONVERT_TO_DRAFT_EVENT", "READY_FOR_REVIEW_EVENT", "HEAD_REF_DELETED_EVENT", "REVIEW_REQUESTED_EVENT"];
const ISSUE_NODES = `__typename
  ... on ClosedEvent { ${ACTOR} } ... on ReopenedEvent { ${ACTOR} }
  ... on LabeledEvent { ${ACTOR} label { name } } ... on UnlabeledEvent { ${ACTOR} label { name } }
  ... on AssignedEvent { ${ACTOR} assignee { ... on User { login } ... on Bot { login } } } ... on UnassignedEvent { ${ACTOR} assignee { ... on User { login } ... on Bot { login } } }
  ... on MilestonedEvent { ${ACTOR} milestoneTitle } ... on DemilestonedEvent { ${ACTOR} milestoneTitle }
  ... on RenamedTitleEvent { ${ACTOR} previousTitle currentTitle }
  ... on CrossReferencedEvent { ${ACTOR} isCrossRepository source { __typename ... on Issue { number repository { nameWithOwner } } ... on PullRequest { number repository { nameWithOwner } } } }
  ... on LockedEvent { ${ACTOR} lockReason } ... on UnlockedEvent { ${ACTOR} }`;
const PR_NODES = `${ISSUE_NODES}
  ... on MergedEvent { ${ACTOR} commit { oid } } ... on ConvertToDraftEvent { ${ACTOR} } ... on ReadyForReviewEvent { ${ACTOR} }
  ... on HeadRefDeletedEvent { ${ACTOR} headRefName } ... on ReviewRequestedEvent { ${ACTOR} requestedReviewer { ... on User { login } } }`;
const timelineTypes = (kind: Kind) => (kind === "issue" ? ISSUE_TYPES : PR_TYPES).join(", ");
const timelineNodes = (kind: Kind) => (kind === "issue" ? ISSUE_NODES : PR_NODES);
const COMMENT_NODES = "id author { login } body createdAt updatedAt url";
const REVIEW_NODES = "id author { login } state body submittedAt";
const PAGE = "pageInfo { hasNextPage endCursor }";

const common = (kind: Kind) => `id number title body state createdAt updatedAt closedAt locked url author { login } milestone { number }
  assignees(first: 10) { nodes { login } } reactions { totalCount }
  labels(first: 50) { ${PAGE} nodes { name } }
  comments(first: 50) { totalCount ${PAGE} nodes { ${COMMENT_NODES} } }
  timelineItems(first: 30, itemTypes: [${timelineTypes(kind)}]) { ${PAGE} nodes { ${timelineNodes(kind)} } }`;
const ISSUE_ONLY = "stateReason";
const PR_ONLY = `isDraft merged mergedAt mergedBy { login } baseRefName headRefName headRefOid additions deletions changedFiles reviewDecision commits { totalCount }
  reviews(first: 30) { totalCount ${PAGE} nodes { ${REVIEW_NODES} } }`;

function pageQuery(kind: Kind): string {
  const conn = kind === "issue" ? "issues" : "pullRequests";
  return `query($owner: String!, $name: String!, $first: Int!, $after: String) {
    repository(owner: $owner, name: $name) { items: ${conn}(first: $first, after: $after, orderBy: { field: UPDATED_AT, direction: DESC }) {
      ${PAGE} nodes { ${common(kind)} ${kind === "issue" ? ISSUE_ONLY : PR_ONLY} } } }
    rateLimit { cost remaining }
  }`;
}

type Field = "comments" | "reviews" | "timelineItems";

/** The rest of a connection that did not fit in the first page of an item. */
async function drain<T>(gh: GitHub, id: string, kind: Kind, field: Field, first: Conn<T>, cap = Infinity): Promise<T[]> {
  const out = [...first.nodes];
  let after = first.pageInfo.endCursor;
  let more = first.pageInfo.hasNextPage;
  const nodes = field === "comments" ? COMMENT_NODES : field === "reviews" ? REVIEW_NODES : timelineNodes(kind);
  const args = field === "timelineItems" ? `itemTypes: [${timelineTypes(kind)}], ` : "";
  const on = (type: string) => `... on ${type} { c: ${field}(${args}first: 100, after: $after) { ${PAGE} nodes { ${nodes} } } }`;
  const query = `query($id: ID!, $after: String) { node(id: $id) { ${kind === "issue" ? on("Issue") : on("PullRequest")} } rateLimit { cost } }`;
  let size = 100;
  for (let pages = 0; more && pages < cap; pages++) {
    let d;
    try {
      d = await gh.graphql<{ node: { c: Conn<T> } | null }>(query.replace("first: 100", `first: ${size}`), { id, after });
    } catch (e) {
      if (e instanceof GitHubError && (e.type === "TIMEOUT" || e.status >= 500) && size > 10) {
        size = Math.max(10, size >> 1);
        pages--;
        continue;
      }
      throw e;
    }
    if (!d.node) break;
    out.push(...d.node.c.nodes);
    after = d.node.c.pageInfo.endCursor;
    more = d.node.c.pageInfo.hasNextPage;
  }
  return out;
}

const login = (a: Login | null | undefined) => a?.login ?? null;

/** Compact type specific JSON of a timeline event. */
export function eventData(e: EventNode): Record<string, unknown> {
  const d: Record<string, unknown> = {};
  const label = (e.label as { name: string } | undefined)?.name;
  if (label) d.label = label;
  const assignee = (e.assignee as Login | undefined)?.login;
  if (assignee) d.assignee = assignee;
  const reviewer = (e.requestedReviewer as Login | undefined)?.login;
  if (reviewer) d.reviewer = reviewer;
  for (const k of ["milestoneTitle", "previousTitle", "currentTitle", "lockReason", "headRefName", "isCrossRepository"]) if (e[k] !== undefined && e[k] !== null) d[k] = e[k];
  const commit = (e.commit as { oid: string } | null | undefined)?.oid;
  if (commit) d.commit = commit;
  const src = e.source as { __typename: string; number?: number; repository?: { nameWithOwner: string } } | undefined;
  if (src?.number) d.source = { type: src.__typename, number: src.number, repo: src.repository?.nameWithOwner };
  return d;
}

export interface Full {
  item: ItemNode;
  comments: CommentNode[];
  reviews: ReviewNode[];
  events: EventNode[];
}

export function storeItem(db: Database, repo: string, kind: Kind, f: Full): void {
  const n = f.item;
  const assignees = n.assignees.nodes.map(a => a.login).join(",");
  tx(db, () => {
    if (kind === "issue") {
      db.query(
        `INSERT OR REPLACE INTO issues (repo, number, node_id, title, body, state, state_reason, author, created_at, updated_at, closed_at, locked, milestone, comments_count, reactions, url, assignees)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
      ).run(repo, n.number, n.id, n.title, n.body, n.state, n.stateReason ?? null, login(n.author), n.createdAt, n.updatedAt, n.closedAt, +n.locked, n.milestone?.number ?? null, n.comments.totalCount, n.reactions.totalCount, n.url, assignees);
    } else {
      db.query(
        `INSERT OR REPLACE INTO pulls (repo, number, node_id, title, body, state, author, created_at, updated_at, closed_at, locked, milestone, comments_count, reactions, url, assignees,
           is_draft, merged, merged_at, merged_by, base_ref, head_ref, head_sha, additions, deletions, changed_files, commits_count, review_decision, reviews_count)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
      ).run(
        repo, n.number, n.id, n.title, n.body, n.state, login(n.author), n.createdAt, n.updatedAt, n.closedAt, +n.locked, n.milestone?.number ?? null, n.comments.totalCount, n.reactions.totalCount, n.url, assignees,
        +!!n.isDraft, +!!n.merged, n.mergedAt ?? null, login(n.mergedBy), n.baseRefName ?? null, n.headRefName ?? null, n.headRefOid ?? null, n.additions ?? null, n.deletions ?? null, n.changedFiles ?? null,
        n.commits?.totalCount ?? null, n.reviewDecision ?? null, n.reviews?.totalCount ?? 0,
      );
    }
    for (const t of ["item_labels", "comments", "events"]) db.query(`DELETE FROM ${t} WHERE repo = ? AND kind = ? AND number = ?`).run(repo, kind, n.number);
    const label = db.query("INSERT OR IGNORE INTO item_labels (repo, kind, number, label) VALUES (?, ?, ?, ?)");
    for (const l of n.labels.nodes.filter(Boolean)) label.run(repo, kind, n.number, l.name);
    const comment = db.query("INSERT OR REPLACE INTO comments (repo, kind, number, id, author, body, created_at, updated_at, url) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)");
    for (const c of f.comments) comment.run(repo, kind, n.number, c.id, login(c.author), c.body, c.createdAt, c.updatedAt, c.url);
    const event = db.query("INSERT OR REPLACE INTO events (repo, kind, number, id, type, actor, created_at, data) VALUES (?, ?, ?, ?, ?, ?, ?, ?)");
    for (const e of f.events) event.run(repo, kind, n.number, e.id, e.__typename, login(e.actor), e.createdAt, JSON.stringify(eventData(e)));
    if (kind === "pr") {
      db.query("DELETE FROM reviews WHERE repo = ? AND number = ?").run(repo, n.number);
      const review = db.query("INSERT OR REPLACE INTO reviews (repo, number, id, author, state, body, submitted_at) VALUES (?, ?, ?, ?, ?, ?, ?)");
      for (const r of f.reviews) review.run(repo, n.number, r.id, login(r.author), r.state, r.body, r.submittedAt);
    }
  });
}

/** Complete one item: drain whatever connections overflowed their first page. */
export async function completeItem(gh: GitHub, kind: Kind, n: ItemNode): Promise<Full> {
  const [comments, reviews, events] = await Promise.all([
    n.comments.pageInfo.hasNextPage ? drain(gh, n.id, kind, "comments", n.comments) : n.comments.nodes,
    n.reviews?.pageInfo.hasNextPage ? drain(gh, n.id, kind, "reviews", n.reviews) : (n.reviews?.nodes ?? []),
    n.timelineItems.pageInfo.hasNextPage ? drain(gh, n.id, kind, "timelineItems", n.timelineItems, 10) : n.timelineItems.nodes,
  ]);
  // GitHub answers null for a comment, review or event it cannot resolve; those are dropped.
  const keep = <T>(xs: (T | null)[]): T[] => xs.filter((x): x is T => x !== null && x !== undefined);
  return { item: n, comments: keep(comments), reviews: keep(reviews), events: keep(events) };
}

export interface ItemsOptions {
  /** Crawl back to this updatedAt instead of the stored watermark. */
  since?: string;
  pageSize?: number;
  log?: (line: string) => void;
}

/**
 * Crawl one repository's issues or pull requests, newest update first, down to the watermark of the previous complete crawl.
 * The page cursor is stored after every page, so an interrupted crawl resumes where it stopped.
 */
export async function syncItems(gh: GitHub, db: Database, owner: string, name: string, kind: Kind, o: ItemsOptions = {}): Promise<{ stored: number }> {
  const repo = `${owner}/${name}`;
  const key = kind === "issue" ? "issues" : "pulls";
  const prev = getState(db, repo, key);
  const resuming = prev.status === "running" && prev.cursor !== null;
  const floor = o.since ?? prev.until;
  let high = resuming ? prev.high : null;
  let stored = 0;
  setState(db, repo, key, { status: "running", cursor: resuming ? prev.cursor : null, high });
  const query = pageQuery(kind);
  await paginate<ItemNode>(
    o.pageSize ?? 50,
    async (after, first) => {
      const { data, errors } = await gh.graphqlPartial<{ repository: { items: { pageInfo: { hasNextPage: boolean; endCursor: string }; nodes: (ItemNode | null)[] } } }>(query, { owner, name, first, after });
      const c = data.repository.items;
      const broken = c.nodes.filter(n => !n).length;
      // GitHub cannot resolve a few items ("Something went wrong"): shrink the page until the broken one stands alone, then skip it.
      if (broken && first > 1) throw new GitHubError(errors[0]?.message ?? "unresolvable item", 200, "TIMEOUT");
      if (broken) o.log?.(`${repo} ${key}: skipped an item GitHub cannot resolve (${errors[0]?.message.slice(0, 60)})`);
      return { items: c.nodes.filter((n): n is ItemNode => !!n), next: c.pageInfo.hasNextPage ? c.pageInfo.endCursor : null };
    },
    async (items, next) => {
      const fulls = await Promise.all(items.map(n => completeItem(gh, kind, n)));
      tx(db, () => fulls.forEach(f => storeItem(db, repo, kind, f)));
      stored += items.length;
      for (const n of items) if (!high || n.updatedAt > high) high = n.updatedAt;
      setState(db, repo, key, { status: "running", cursor: next, high });
      o.log?.(`${repo} ${key}: ${stored} stored${next ? "" : " (end)"}`);
      const oldest = items.at(-1)?.updatedAt;
      return !(floor && oldest && oldest < floor);
    },
    resuming ? prev.cursor : null,
  );
  const partial = o.since !== undefined && (!prev.until || o.since > prev.until);
  setState(db, repo, key, { status: "idle", cursor: null, high: null, until: partial ? prev.until : (high && prev.until && high < prev.until ? prev.until : (high ?? prev.until)) });
  return { stored };
}
