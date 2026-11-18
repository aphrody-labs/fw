# Porting the live cloud from aphrody-labs/ay

The facts below are the live state the port must reflect. They were reported by the session that owns the
Cloudflare account and the fleet, on 2026-11-16. The old `device/cloudflare/cloudflare.toml` no longer matches them:
never `cf.ts apply tunnels` from it.

## Live Cloudflare state
- **Tunnel `aphrody`, ingress v13:** the 3 standby rules,
  `mcp-origin.aphrody.com → http://127.0.0.1:8790` (the ai-os, on <prod-host>),
  `ssh-<prod-host>.aphrody.com → ssh://127.0.0.1:22`, then a 404 catch-all.
  - Earlier today an overwrite had left only the standby rules, so `mcp-origin` returned the 404.
- **<prod-host> is the only connector:** the aphrody connector replicas on <build-host> and <test-host> (`cloudflared-mig-<prod-host>`) are disabled, because the loopback origins (the ai-os on 8790) exist only on <prod-host>. With <prod-host> alone, `mcp-origin.aphrody.com` answers 200 on 8/8 requests; with the replicas it returned 502 on about 5/6. Re-enable replicas only once the cluster load-balancer ports exist on every host.
- **Account hardening done:** observability is on for 5 Workers, workers.dev is off for the gateway, minimum TLS is 1.2 on 3 R2 custom domains, and the legacy ftp DNS records are removed.
- **Tunnel <test-host> removed:** the tunnel and its connectors (`cloudflared-<test-host>`, `cloudflared-prod`) are gone from the account and the hosts.
- **Bucket deleted:** `aphrody-archive` no longer exists.
- **Undeclared ports:** the cluster load-balancer ports declared in the old file (19300, 19790, 19080, …) don't exist on any host.

## What the port must carry
- **The `aphrody-sites` deploy** (`edge.yml`: build `apps/r2-site/build.ts`, then `wrangler deploy --config apps/r2-site/wrangler/aphrody-sites.toml`):
  - deploy from `main` only;
  - the token is scoped to the deploy step;
  - secrets `CLOUDFLARE_API_TOKEN` and `CLOUDFLARE_ACCOUNT_ID` are already set on aphrody-dev/ay.
- **Gateway hardening** (`~/.claude/tmp/gateway-hardening.patch` for `aphrody-mcp-gateway/index.ts`):
  - security headers on every response;
  - 405 with `Allow` on wrong methods;
  - JSON 401;
  - `decodeURIComponent` and origin-fetch failures turn into 502;
  - an R2 `onlyIf` miss turns into 304.
- **`entry.cf.ts`:** answer 503 when the `env[site.bucket]` binding is missing.
- **`serve.ts`:**
  - 416 on an unsatisfiable Range (today it answers 500);
  - 412 on a failing If-Match (today it answers 304);
  - an ETag on live 200 responses.
- **Fleet scripts** (`fleet-backup.sh`, `fleet-rebuild.sh`):
  - `set -euo pipefail`;
  - errors go to stderr;
  - a failed SQLite `.backup` fails the run.
- **Cache Rules:** the ones on R2-Worker hosts are dead config; drop them.
