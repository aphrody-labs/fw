#!/usr/bin/env bun
/**
 * Real-time Zed knowledge importer for ~/.gemini/brain/zed/.
 * Fetches live GitHub (issues, pulls, discussions, actions, project #78, security advisories,
 * releases, repo insights) and zed.dev (/blog, /about, /roadmap, /theme-builder, /languages),
 * writes compact Markdown digests, and optionally re-indexes FTS5 via sync.ts.
 *
 * Usage:
 *   bun ~/.gemini/scripts/import-zed-brain.ts --live
 *   bun ~/.gemini/scripts/import-zed-brain.ts --live --sync
 */

import { mkdirSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

const GEMINI_DIR = join(homedir(), ".gemini");
const BRAIN_DIR = join(GEMINI_DIR, "brain", "zed");
const UA = "aphrody-zed-brain-sync/1.0";

function ghApi<T = unknown>(endpoint: string): T | null {
  const proc = Bun.spawnSync(["gh", "api", endpoint], { stdout: "pipe", stderr: "pipe" });
  if (proc.exitCode !== 0) return null;
  try {
    return JSON.parse(proc.stdout.toString()) as T;
  } catch {
    return null;
  }
}

function ghGraphql<T = unknown>(query: string): T | null {
  const proc = Bun.spawnSync(["gh", "api", "graphql", "-f", `query=${query}`], { stdout: "pipe", stderr: "pipe" });
  if (proc.exitCode !== 0) return null;
  try {
    return JSON.parse(proc.stdout.toString()) as T;
  } catch {
    return null;
  }
}

function decodeHtml(s: string): string {
  return s
    .replace(/&amp;/g, "&")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&quot;/g, '"')
    .replace(/&#39;|&#x27;/g, "'")
    .replace(/&#8211;|&ndash;/g, "-")
    .replace(/&#8212;|&mdash;/g, "-")
    .replace(/&#8217;|&rsquo;/g, "'")
    .replace(/&nbsp;/g, " ")
    .replace(/\s+/g, " ")
    .trim();
}

async function fetchHtml(url: string): Promise<{ status: number; html: string }> {
  try {
    const res = await fetch(url, { headers: { "User-Agent": UA }, redirect: "follow" });
    return { status: res.status, html: await res.text() };
  } catch {
    return { status: 0, html: "" };
  }
}

function extractHeadings(html: string, limit = 18): string[] {
  const raw = [...html.matchAll(/<h[123][^>]*>([\s\S]*?)<\/h[123]>/gi)]
    .map((m) => decodeHtml(m[1].replace(/<[^>]+>/g, " ")))
    .filter((s) => s.length >= 3 && s.length <= 140);
  return [...new Set(raw)].slice(0, limit);
}

function extractBlogPosts(html: string, limit = 15): Array<{ href: string; title: string }> {
  const seen = new Set<string>();
  const posts: Array<{ href: string; title: string }> = [];
  for (const m of html.matchAll(/<a[^>]+href="(\/blog\/[^"#?]+)"[^>]*>([\s\S]*?)<\/a>/gi)) {
    const href = `https://zed.dev${m[1]}`;
    const text = decodeHtml(m[2].replace(/<[^>]+>/g, " "));
    if (!seen.has(href) && text.length >= 6 && text.length <= 160) {
      seen.add(href);
      posts.push({ href, title: text });
      if (posts.length >= limit) break;
    }
  }
  return posts;
}

function extractLanguages(html: string): string[] {
  const langs = new Set<string>();
  for (const m of html.matchAll(/href="\/languages\/([a-z0-9_+-]+)"/gi)) {
    langs.add(m[1]);
  }
  return [...langs].sort();
}

async function main() {
  mkdirSync(BRAIN_DIR, { recursive: true });
  const now = new Date().toISOString();

  const [
    repo,
    releases,
    issuesRaw,
    pullsRaw,
    workflowsRaw,
    advisoriesRaw,
    gqlDiscussions,
    project78Page,
    blogPage,
    aboutPage,
    roadmapPage,
    themePage,
    languagesPage,
  ] = await Promise.all([
    Promise.resolve(ghApi<any>("repos/zed-industries/zed")),
    Promise.resolve(ghApi<any[]>("repos/zed-industries/zed/releases?per_page=10")),
    Promise.resolve(ghApi<any[]>("repos/zed-industries/zed/issues?state=open&sort=updated&direction=desc&per_page=20")),
    Promise.resolve(ghApi<any[]>("repos/zed-industries/zed/pulls?state=open&sort=updated&direction=desc&per_page=15")),
    Promise.resolve(ghApi<any>("repos/zed-industries/zed/actions/workflows?per_page=100")),
    Promise.resolve(ghApi<any[]>("repos/zed-industries/zed/security-advisories?per_page=15")),
    Promise.resolve(
      ghGraphql<any>(`{
        repository(owner: "zed-industries", name: "zed") {
          issues(states: OPEN) { totalCount }
          pullRequests(states: OPEN) { totalCount }
          discussions(first: 10, orderBy: {field: UPDATED_AT, direction: DESC}) {
            totalCount
            nodes { number title url updatedAt category { name } }
          }
        }
      }`),
    ),
    fetchHtml("https://github.com/orgs/zed-industries/projects/78"),
    fetchHtml("https://zed.dev/blog"),
    fetchHtml("https://zed.dev/about"),
    fetchHtml("https://zed.dev/roadmap"),
    fetchHtml("https://zed.dev/theme-builder"),
    fetchHtml("https://zed.dev/languages"),
  ]);

  const openIssues = (issuesRaw ?? []).filter((i) => !i.pull_request).slice(0, 12);
  const openPrs = (pullsRaw ?? []).slice(0, 12);
  const discussions = gqlDiscussions?.data?.repository?.discussions?.nodes ?? [];
  const discussionsTotal = gqlDiscussions?.data?.repository?.discussions?.totalCount ?? null;
  const openIssuesTotal = gqlDiscussions?.data?.repository?.issues?.totalCount ?? null;
  const openPrsTotal = gqlDiscussions?.data?.repository?.pullRequests?.totalCount ?? null;
  const workflows = workflowsRaw?.workflows ?? [];
  const activeWorkflows = workflows.filter((w: any) => w.state === "active");
  const advisories = advisoriesRaw ?? [];

  const projTitle = decodeHtml(project78Page.html.match(/<title[^>]*>([^<]+)<\/title>/i)?.[1] ?? "zed-industries/projects/78");
  const blogPosts = extractBlogPosts(blogPage.html, 15);
  const aboutHeadings = extractHeadings(aboutPage.html, 12);
  const roadmapHeadings = extractHeadings(roadmapPage.html, 20);
  const themeHeadings = extractHeadings(themePage.html, 10);
  const languages = extractLanguages(languagesPage.html);

  const liveSnapshot = [
    `# Zed GitHub Real-Time Snapshot`,
    ``,
    `- **Fetched**: \`${now}\``,
    `- **Repository**: \`zed-industries/zed\` | Stars: **${repo?.stargazers_count ?? "?"}** | Forks: **${repo?.forks_count ?? "?"}** | Open Issues: **${openIssuesTotal ?? "?"}** | Open PRs: **${openPrsTotal ?? "?"}** | Discussions: **${discussionsTotal ?? "?"}** | Last Push: \`${repo?.pushed_at ?? "?"}\``,
    `- **Active Actions Workflows**: **${activeWorkflows.length}** / ${workflowsRaw?.total_count ?? workflows.length}`,
    `- **Project #78**: \`${projTitle}\` (HTTP ${project78Page.status}, https://github.com/orgs/zed-industries/projects/78)`,
    ``,
    `## Latest Releases`,
    ...(releases ?? []).slice(0, 8).map((r) => `- \`${r.tag_name}\` (${r.prerelease ? "preview" : "stable"}, ${r.published_at?.slice(0, 10) ?? "?"})`),
    ``,
    `## Recent Open Issues`,
    ...openIssues.map((i) => `- #${i.number}: ${i.title} (${i.updated_at?.slice(0, 10)})`),
    ``,
    `## Recent Open Pull Requests`,
    ...openPrs.map((p) => `- #${p.number}${p.draft ? " [draft]" : ""}: ${p.title} (${p.updated_at?.slice(0, 10)})`),
    ``,
    `## Recent Discussions`,
    ...discussions.map((d: any) => `- #${d.number} [${d.category?.name ?? "General"}]: ${d.title} (${d.updatedAt?.slice(0, 10)})`),
    ``,
    `## Security Advisories (GitHub Security)`,
    ...advisories.slice(0, 10).map((a) => `- \`${a.ghsa_id}\` (**${a.severity}**, ${a.published_at?.slice(0, 10) ?? "?"}): ${a.summary}`),
    ``,
    `## GitHub Actions Workflows (Active Sample)`,
    ...activeWorkflows.slice(0, 25).map((w: any) => `- \`${w.path}\` — ${w.name}`),
    ``,
  ].join("\n");

  const zedDevMd = [
    `# zed.dev Real-Time Snapshot`,
    ``,
    `- **Fetched**: \`${now}\``,
    `- **Pages Checked**: \`/blog\` (${blogPage.status}), \`/about\` (${aboutPage.status}), \`/roadmap\` (${roadmapPage.status}), \`/theme-builder\` (${themePage.status}), \`/languages\` (${languagesPage.status})`,
    ``,
    `## Blog (\`https://zed.dev/blog\`)`,
    ...blogPosts.map((p) => `- [${p.title}](${p.href})`),
    ``,
    `## Roadmap (\`https://zed.dev/roadmap\`)`,
    ...roadmapHeadings.map((h) => `- ${h}`),
    ``,
    `## About & Vision (\`https://zed.dev/about\`)`,
    ...aboutHeadings.map((h) => `- ${h}`),
    ``,
    `## Theme Builder (\`https://zed.dev/theme-builder\`)`,
    ...themeHeadings.map((h) => `- ${h}`),
    ``,
    `## Supported Languages (\`https://zed.dev/languages\` — ${languages.length} indexed slugs)`,
    languages.length ? languages.map((l) => `\`${l}\``).join(", ") : "(dynamic JS list — see https://zed.dev/languages)",
    ``,
  ].join("\n");

  const indexMd = [
    `# Zed Unified Real-Time Brain Index`,
    ``,
    `- **Last Live Refresh**: \`${now}\``,
    `- **Refresh Command**: \`bun ~/.gemini/scripts/import-zed-brain.ts --live --sync\``,
    `- **Latest Releases**: stable \`${(releases ?? []).find((r) => !r.prerelease)?.tag_name ?? "?"}\` | preview \`${(releases ?? []).find((r) => r.prerelease)?.tag_name ?? "?"}\``,
    `- **GitHub Pulse**: ${repo?.stargazers_count ?? "?"}★ | ${repo?.forks_count ?? "?"} forks | ${openIssuesTotal ?? "?"} open issues | ${openPrsTotal ?? "?"} open PRs | ${discussionsTotal ?? "?"} discussions | ${activeWorkflows.length} active workflows | ${advisories.length} recent GHSAs`,
    ``,
    `## Knowledge Files`,
    `- \`LIVE_SNAPSHOT.md\` — Live GitHub Issues, PRs, Discussions, Actions, Project #78, Security Advisories, Releases, Insights`,
    `- \`ZED_DEV.md\` — Live \`zed.dev\` Blog (Delta, Xanadu, Sandboxing, etc.), Roadmap, About, Theme Builder, Languages`,
    `- \`../zed-ai/INDEX.md\` + \`plugins/aphrody/skills/zed-ai/SKILL.md\` + \`plugins/aphrody/docs/zed/ai/\` (28 upstream AI docs)`,
    `- \`../zed-nix/INDEX.md\` + \`plugins/aphrody/skills/zed-nix/SKILL.md\` + \`plugins/aphrody/docs/zed/nix/\` (14 upstream Nix files)`,
    `- \`plugins/aphrody/skills/zed/SKILL.md\` — Core Zed architecture & Aphrody compatibility invariants`,
    ``,
  ].join("\n");

  writeFileSync(join(BRAIN_DIR, "LIVE_SNAPSHOT.md"), liveSnapshot, "utf8");
  writeFileSync(join(BRAIN_DIR, "ZED_DEV.md"), zedDevMd, "utf8");
  writeFileSync(join(BRAIN_DIR, "INDEX.md"), indexMd, "utf8");

  if (process.argv.includes("--sync")) {
    const syncProc = Bun.spawnSync(["bun", join(GEMINI_DIR, "scripts", "sync.ts")], { stdout: "inherit", stderr: "inherit" });
    if (syncProc.exitCode !== 0) process.exit(syncProc.exitCode);
  }

  console.log(
    JSON.stringify(
      {
        ok: true,
        fetchedAt: now,
        stars: repo?.stargazers_count,
        openIssuesTotal,
        openPrsTotal,
        discussionsTotal,
        activeWorkflows: activeWorkflows.length,
        advisories: advisories.length,
        blogPosts: blogPosts.length,
        languages: languages.length,
        project78: { status: project78Page.status, title: projTitle },
        files: ["INDEX.md", "LIVE_SNAPSHOT.md", "ZED_DEV.md"],
      },
      null,
      2,
    ),
  );
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
