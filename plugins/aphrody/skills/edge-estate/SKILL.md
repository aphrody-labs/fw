---
name: edge-estate
description: "Edit, plan, apply and check the edge estate (Cloudflare zones, DNS, Worker routes and domains, R2, tunnels) and read OVH and Hostinger facts. Use before touching any DNS record, subdomain, route, Worker, R2 bucket or registrar setting; never edit these by hand in a dashboard or API call."
---

# Edge estate

`cluster/estate.toml` is the single source of names; `fw estate` (crate `crates/fw-estate`, binary `fw-estate`) diffs it against live Cloudflare and applies the difference. Heavy builds run on fr1; the credentials stay in the environment.

## Commands

- `fw estate plan [--estate cluster/estate.toml]`: writes `cluster/estate.plan.json` (every create, update, delete, with the live ids) and prints it.
- `fw estate apply --committed-in <checkout>`: runs only a plan that is on `origin/main` of that checkout (or tracked and unmodified in the plan's own checkout) and equal to a fresh plan; prints `remaining actions after apply: N` (0 is done).
- `fw estate check [--providers]`: best-practice findings, each with its source document; `--providers` adds OVH registrar and Hostinger facts. Exit 1 on an error finding.
- `fw estate import`: prints zone sections generated from live state (use when adopting a zone).
- `fw estate registrar-ds <zone>`: publishes the Cloudflare DS record at the OVH registrar (zones whose registrar account is the OVH account in `~/.ovh.conf`).
- `fw estate backup` and `backup-units`: D1 exports and an rclone mirror of the R2 buckets onto the `[backup]` host, with a dated trash for deleted objects.

## Workflow

1. Edit `cluster/estate.toml` only: a record is `"TYPE name [proxied] [prio=N] [ttl=N] | content"`, content `tunnel:<name>` or `worker`.
2. `fw estate plan`, read the deletions, commit and push `estate.toml` and `estate.plan.json` together.
3. `fw estate apply`, then prove by content (rendered page or API payload), never a status code.
4. A deletion is a record, route or domain the zone no longer lists, so it exists in the plan before it runs. Buckets are never deleted by apply; a Worker only when listed under `[retired.worker]` and it owns no Durable Object.
5. Anything that recreates records (deploy scripts, `cf.toml`) must lose the name too, or it comes back: `learn.aphrody.com` was recreated by `aphrody/tooling/bun/cloudflare-deploy.ts`.

## Facts that are easy to get wrong

- Dead origins: a proxied `AAAA 100::` needs a Worker route or custom domain (else 522); a tunnel CNAME needs an ingress rule in that tunnel (else the catch-all 404); a Worker custom domain wants no other record.
- The TLS 1.2 share is measured with GraphQL `httpRequests1dGroups.sum.clientSSLMap` before raising `min_tls_version`.
- R2 buckets live in WEUR; the abort-incomplete-multipart lifecycle rule is declared per bucket.
- D1 export needs a token with D1 Write; the dedicated backup token carries only that.
- OVH: credentials in `~/.ovh.conf` (`[default] endpoint`, then the `ovh-eu` section with application_key, application_secret, consumer_key); signature `$1$` + sha1 of `secret+consumer+method+url+body+timestamp`, timestamp corrected with `/auth/time`. The account holds `aphrody.com` (registrar) and `rpbey.fr`, one VPS (fr1) and the mail redirection; `dragonballfr.com`, `rosegriffon.fr` and `ostendis.fr` are registered at OVH under another customer account, so their DS records cannot be published from here.
- Hostinger: only the prd VPS and the domain `3dland.tech`; no DNS zones of ours and no firewall group on prd.
- Self-hosted mode of each service is described in `docs/research/cloudflare-dual-mode.md`.
