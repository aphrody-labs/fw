/** Releases with notes and asset metadata (names, sizes, download counts). */
import type { Database } from "bun:sqlite";
import { GitHub, paginate } from "./github.ts";
import { tx } from "./db.ts";

interface AssetNode { name: string; size: number; downloadCount: number; contentType: string; createdAt: string; downloadUrl: string }
export interface ReleaseNode {
  tagName: string;
  name: string | null;
  description: string | null;
  isPrerelease: boolean;
  isDraft: boolean;
  isLatest: boolean;
  createdAt: string;
  publishedAt: string | null;
  url: string;
  author: { login: string } | null;
  releaseAssets: { nodes: AssetNode[] };
}

const QUERY = /* graphql */ `query($owner: String!, $name: String!, $first: Int!, $after: String) {
  repository(owner: $owner, name: $name) { releases(first: $first, after: $after, orderBy: { field: CREATED_AT, direction: DESC }) {
    pageInfo { hasNextPage endCursor }
    nodes { tagName name description isPrerelease isDraft isLatest createdAt publishedAt url author { login }
      releaseAssets(first: 100) { nodes { name size downloadCount contentType createdAt downloadUrl } } } } }
  rateLimit { cost remaining }
}`;

/** All releases of a repository, replaced as a whole (they are few and editable). */
export async function syncReleases(gh: GitHub, db: Database, owner: string, name: string): Promise<number> {
  const repo = `${owner}/${name}`;
  const all: ReleaseNode[] = [];
  await paginate<ReleaseNode>(
    50,
    async (after, first) => {
      const d = await gh.graphql<{ repository: { releases: { pageInfo: { hasNextPage: boolean; endCursor: string }; nodes: ReleaseNode[] } } }>(QUERY, { owner, name, first, after });
      const r = d.repository.releases;
      return { items: r.nodes, next: r.pageInfo.hasNextPage ? r.pageInfo.endCursor : null };
    },
    items => void all.push(...items),
  );
  tx(db, () => {
    db.query("DELETE FROM releases WHERE repo = ?").run(repo);
    db.query("DELETE FROM release_assets WHERE repo = ?").run(repo);
    const rel = db.query("INSERT OR REPLACE INTO releases (repo, tag, name, body, is_prerelease, is_draft, is_latest, created_at, published_at, author, url) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)");
    const asset = db.query("INSERT OR REPLACE INTO release_assets (repo, tag, name, size, downloads, content_type, created_at, url) VALUES (?, ?, ?, ?, ?, ?, ?, ?)");
    for (const r of all) {
      rel.run(repo, r.tagName, r.name, r.description, +r.isPrerelease, +r.isDraft, +r.isLatest, r.createdAt, r.publishedAt, r.author?.login ?? null, r.url);
      for (const a of r.releaseAssets.nodes) asset.run(repo, r.tagName, a.name, a.size, a.downloadCount, a.contentType, a.createdAt, a.downloadUrl);
    }
  });
  return all.length;
}
