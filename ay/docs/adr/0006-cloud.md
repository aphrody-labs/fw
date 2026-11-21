# ADR-6: Cloud on a Bun edge runtime with a Workers-compatible API

Status: proposed, 2026-11-16.

## Decision (proposed)
A Bun-based edge runtime exposing the Workers API surface (fetch handler, KV, object storage, queues, cron), a
git-push deploy pipeline with preview URLs, custom domains with ACME TLS, logs, and a dashboard. It runs on the
production host and hosts Aphrody Search and the brand site. Existing Cloudflare and Vercel deployments keep running
until each one is moved and proven.

## Carried over from the archive
The aphrody-sites Worker deploy (`edge.yml`: build in `apps/r2-site/build.ts`, `wrangler deploy --config
apps/r2-site/wrangler/aphrody-sites.toml`) and `device/cloudflare/projects.toml` are ported first so the live sites
keep redeploying.
