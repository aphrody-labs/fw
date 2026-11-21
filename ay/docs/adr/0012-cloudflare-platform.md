# ADR-12: Cloudflare as the edge of every Aphrody product, and `ay` shaped like Cloudflare's own tooling

Status: proposed, 2026-11-17. Extends 0006 and 0010; changes nothing on the frozen zones (aphrody.com, aphrody.app DNS, zone and routes).

## Context
Read live on 2026-11-17 (the account inventory itself is private: `.private/research/cloudflare-inventory.md`).
- The account is on the Workers Free plan with R2 paid. A Worker on Free stops answering at its daily request limit with error 1027,
  on workers.dev and probably on custom routes. One Worker (the GitHub events receiver) produces most of the account's requests, so the
  release Worker is refused for a reason that has nothing to do with releases.
- Cloudflare's own organization ships what only Cloudflare can offer at the edge: `kitesurf` (browser engine on Dynamic Workers), `browser-run`
  (puppeteer/playwright on Browser Run), `agents`, `computer` and `cloudflare-os` (an OS-shaped platform on Durable Objects), `sandbox-sdk` and
  `containers`, `workers-rs`, `cloudflared`, `quiche`, `boringtun`, `pingora`, `lol-html`, `web-bot-auth`, `artifact-fs`, `serverless-registry`, `ci`.
  Containers, Pipelines and Dynamic Workers need Workers Paid.

## Decision
1. One service per product, chosen from what Cloudflare runs natively, each declared in the product's own manifest and deployed by `ay`:
   | Area | Cloudflare service | Aphrody piece |
   |---|---|---|
   | releases | R2 + one Worker (`workers-rs` when the logic grows), `serverless-registry` for images | `ay-tools` release Worker, `aphrody-chromium` bucket |
   | browser | `kitesurf` engine on Dynamic Workers, Browser Run for real Chromium | `browser` (kitesurf submodule), acceptance dashboard |
   | crawl | Browser Run + `agents` (state in Durable Objects) + `lol-html` + `web-bot-auth` (identified crawler) | mirror and crawl of the Bun fork, then the search product |
   | tunnel | `cloudflared` with Access apps in front of every origin | existing tunnels; the one without connections is relaunched or removed |
   | ayvpn | `boringtun`/`quiche` for the data path, `workers-rs` control plane, WARP through Zero Trust; MASQUE only through the documented client | `ayvpn` |
   | main page | one Worker with static assets in front of R2 | www.aphrody.com |
2. Remove the cause of 1027 before buying capacity: the events receiver filters and batches by queue so it stops answering every delivery; the
   Workers Paid plan is the owner's decision (it also unlocks Containers and Dynamic Workers, so `kitesurf` on the edge depends on it).
3. Purge is by inventory, never by name: an item is a purge candidate when nothing in the workspace repositories deploys it and it has had no traffic for 30 days.
   Candidates are listed privately and removed one by one; what moves off the edge to the production host moves only on the owner's explicit go
   for that host (read-only until then), as a dry-run install plan.
4. `ay` follows Cloudflare's tooling shape (`workers-sdk`/wrangler): one manifest per product, `ay dev`, `ay deploy`, `ay tail`, `ay types`,
   preview before production, bindings generated from the manifest. The Bun plugin of ADR-0010 is the Bun face of the same manifest.
5. The organization is unified the way Cloudflare unifies its own: one repository per product with `workers-sdk`-style packages, one toolchain
   (`templates/toolchain.toml`), one release path, the same naming (`naming.toml`), forks as submodules with relative URLs.

## Not done here
No Cloudflare resource was created, changed or removed by this record. The plan is executed unit by unit: the events receiver first (it frees the
release Worker), then the purge list, then the product manifests.
