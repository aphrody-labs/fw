# Lot: the edge and infra estate, redesigned

Order of the owner, 2026-11-21: rethink every DNS record, subdomain, bucket and Worker of aphrody and of the stack. Then deduplicate and reorganize. Install the dependencies, the infra setup, the scripts, the skills and the MCP servers for Cloudflare, OVH and Hostinger. Turn every best practice on and apply it.

This lot runs together with `cloudflare-dual-mode-parity.md`, which covers self-hosted and cloud with proven parity.

## Inventory, live (never from memory)

- **Cloudflare**, read through the Cloudflare API or MCP:
  - zones and DNS records;
  - Workers and routes;
  - R2 buckets, D1, KV, Queues, Durable Objects;
  - Pages, Access, WAF and rulesets, certificates, email routing, tokens (names and scopes only).
- **Hostinger**, read through the Hostinger API (skill `hostinger`): VPS, DNS zones, snapshots, firewall.
- **OVH**, read through the OVH API: domains, DNS, servers.
  - First find whether an account and credentials exist (vault, env names, gh secrets).
  - If none exist, report it as the blocker; do not create an account.
- **Repos:** every declaration of the above in the repos, for example:
  - ay `packages/cf-control-plane/cf.toml`, `apps/edge/wrangler.jsonc`, `edge.toml`;
  - fw `crates/fw-edge`, `cloudflare/*`, `cluster/*.toml`;
  - aphrody `site/`.

## Target

- **One declarative estate file**, the single source of names. It holds zones, subdomains, buckets, Workers, routes and bindings, per environment (cloud and self-hosted). It extends `cf.toml` or replaces it, with the reason given.
  - Hostnames are derived from product names, following the derive-from-code rule.
  - Each hostname has exactly one owner service.
- **Plan and apply:** `fw estate plan|apply|check` (or the existing control plane, if it already does this) diffs live state against the file.
  - Deletions (records, buckets, Workers, routes) are listed in the plan before they are applied, and need the plan to be committed first.
  - Data in buckets is never deleted without an explicit listing in the plan.
- **Best practices, each one stated with its source doc:**
  - **DNS:** DNSSEC, CAA, SPF/DKIM/DMARC.
  - **TLS and HTTP:** HSTS, minimum TLS 1.3 where clients allow, HTTP/3.
  - **Access:** least-privilege API tokens per job.
  - **Workers:** observability and logs on, `compatibility_date` current, smart placement where it helps.
  - **R2:** lifecycle rules, abort-multipart.
  - **Caching:** cache rules.
  - **Security:** WAF managed rules, rate limits on APIs, bot protection where it is free.
  - **Recovery:** backups of D1 and R2 to our own disks.
- **Tooling installed and wired:**
  - wrangler / `cf` CLI;
  - the Cloudflare MCP;
  - an OVH CLI or MCP if one exists upstream, and the Hostinger MCP;
  - skills updated with what is real.

## Proof

Each change is checked against live API responses and rendered pages, never a bare status code. Before and after tables per zone and per service go in `docs/research/edge-estate-2026-11.md`.
