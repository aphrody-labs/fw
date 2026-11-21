---
name: nextjs
description: "Next.js 16.5+ work: App Router architecture and proxy.ts, next.config and caching. Use for any Next.js code, config, performance or debugging task. Cloudflare deployment is the cloudflare"
---
> runtime MCP and devtools, verifying a change in a running dev server, bundle-size audits, adopting Cache Components and Partial Prefetching and optimizing what they surface. plugin's nextjs-on-cloudflare.

# Next.js 16.5+

Architecture and config first (`nextjs-app-router`, `nextjs-config-caching`), then the runtime loop, then the optimization references.

Read the reference that matches the task in full before acting. Paths written inside reference files are relative to this skill's directory (the one holding this SKILL.md). Where a reference names another merged skill, open `references/<name>.md`; its helper files are under `references/<name>/`.

| Reference | Use when |
| --- | --- |
| [references/core.md](references/core.md) | Former body of this skill (read first) |
| [references/nextjs-app-router.md](references/nextjs-app-router.md) | Next.js 16.5+ App Router architecture covering proxy.ts, async Request APIs, PageProps/LayoutProps/RouteContext, next/root-params, RSC vs Client boundaries, useSyncExternalStore, catchError, Parallel Routes default.tsx, and route.go. |
| [references/nextjs-config-caching.md](references/nextjs-config-caching.md) | Next.js 16.5+ next.config.ts schema, rewrites (beforeFiles host matching, afterFiles, fallback), image defaults, Turbopack config, Cache Components ('use cache', cacheLife, cacheTag, revalidateTag, updateTag, io, prefetch, navigation, instant, ensureStatic). |
| [references/nextjs-devtools-mcp.md](references/nextjs-devtools-mcp.md) | Connect to Next.js 16.5+ runtime MCP (/_next/mcp), vercel/next-devtools-mcp, and offline nextjs-brain MCP tools to inspect routes, compilation issues, errors, logs, types, and config schemas. |
| [references/next-dev-loop.md](references/next-dev-loop.md) | Verify Next.js runtime behavior after editing app code. Use this skill to confirm a change actually works in a running app — not just that it compiles or type-checks. Combines /_next/mcp (Next.js's view) with agent-browser (the browser's view). Requires a running `next dev`. |
| [references/next-bundle-optimizer.md](references/next-bundle-optimizer.md) | Audit and reduce Next.js browser initial-load work. Use for slow route startup, oversized client bundles, duplicate browser dependencies, or features that can wait for interaction. |
| [references/next-cache-components-adoption.md](references/next-cache-components-adoption.md) | Turn on Cache Components in a Next.js app and resolve the blocking routes it surfaces. Use when the user wants to enable, adopt, or migrate to Cache Components, flip the `cacheComponents` flag, work through a flood of blocking-prerender / instant validation errors, run the `cache-components-instant-false` codemod, or... |
| [references/next-cache-components-optimizer.md](references/next-cache-components-optimizer.md) | Optimize the meaningful UI available immediately from a Next.js route on an initial load (hard navigation) or named client-side navigation (soft navigation). Encode the goal as a failing @next/playwright instant() e2e and work it to green, one verified route and entry point at a time; the shipped test then guards... |
| [references/next-partial-prefetching-adoption.md](references/next-partial-prefetching-adoption.md) | Turn on Partial Prefetching in a Next.js app and work through the insights it surfaces. Use when the user wants to enable or adopt Partial Prefetching, flip the `partialPrefetching` flag, opt routes in with `export const prefetch = 'partial'`, audit `Link prefetch={true}` behavior, preserve existing prefetched UI with... |
| [references/next-partial-prefetching-optimizer.md](references/next-partial-prefetching-optimizer.md) | Optimize what selected Next.js client navigations include before the click under Partial Prefetching. Use after Cache Components and Partial Prefetching are adopted when the user wants selected URL-specific UI to be instant, wants reusable content to wait for navigation, or needs to choose between default, viewport,... |

<!-- merge-skill-families:v1 -->
