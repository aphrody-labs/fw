# cloudflare-estate-and-shenron-cutover: the Cloudflare estate as code, and the dragonballfr.com cutover (queued)

Phase 0 (production to Cloudflare), then Phase 2 (subdomains, DNS, tunnels, buckets). Owner: the background worker for the fleet build cluster and production move to Cloudflare. Release v2026.11.0: blocks the "custom domains, DNS, TLS, tunnels, R2" line of the release train.

## Goal
Replace dashboard state with OpenTofu in the monorepo's `infra/` (roadmap decision 5), and finish moving production off the VPS.

## Facts
- agy conversation 31b9eccf (2026-11-18) held an inventory of four zones: 188 DNS records, 42 Worker routes, 26 custom domains, 3 `cloudflared` tunnels, 11 R2 buckets. Those counts came from the user's message and were never re-read through the API in that session. Its target repo `ay-cf` was archived on 2026-11-20 (`gh api repos/aphrody-labs/ay-cf`, `archived: true`).
- `cluster/cloudflare-gcloud.toml` in fw configures one Worker, one Durable Object and one bucket; it is not an inventory.
- agy conversations 09b6b6e0 and 0cfb5ed4 (2026-11-18 and 2026-11-01) found static-chunk errors on dragonballfr.com (70 console messages and 130 requests in the 2026-11-18 Chrome pass) and began moving shenron off the VPS. `https://dragonballfr.com/` answered 200 with a 676,904 B body on 2026-11-21; content errors were not re-measured.
- Live Cloudflare-served pages on 2026-11-21: aphrody.app (title "aphrody.app, le lanceur Aphrody"), web.aphrody.com (title "aphrody_desktop — Aphrody WebGPU / Wasm"), material.aphrody.com (JSON index with `/v1/icons`, `/v1/fonts`, `/v1/rust`), learn.aphrody.com.

## Do
1. Read the estate live with the Cloudflare MCP or `cf` (zones, DNS records, Worker routes and custom domains, tunnels, R2 buckets) and write the counts; diff against the agy numbers above.
2. `tofu import` each resource into `infra/cloudflare/`, with SOPS+age secrets; `tofu plan` must show no change.
3. Re-measure dragonballfr.com in Chrome (console errors, failed requests, LCP) and record numbers; fix the chunk errors in `aphrody-labs/shenron`.
4. Cut shenron's production from prd to Cloudflare behind a staged, reversible rollout; keep the VPS copy until the numbers match.

## Acceptance
`tofu plan` clean on the live account; a rendered-page screenshot and a payload check for each public hostname; dragonballfr.com console shows 0 errors.
