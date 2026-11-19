# Workers Paid & R2 Paid FinOps Strategy

## Cost Controls Enforced by `ay-cf`

1. **Observability Event Cap (`20M` free/month)**: High-RPS router/CDN Workers use `logs.head_sampling_rate = 0.1` and `traces.head_sampling_rate = 0.05` (with `traces.enabled = true`); low-volume control/webhook Workers retain `1.0` logs.
2. **CPU Bounding (`limits.cpu_ms`)**: CDN/asset routers are bounded at `50` ms; API/MCP/webhook Workers are bounded at `250`–`500` ms.
3. **Zero-Worker Static Delivery**: Public R2 buckets (`aphrody-assets`, `aphrody-video`, `aphrody-dist`, `aphrody-releases`, `bun-lib`, `rosegriffon-cdn`) are served via R2 Custom Domains backed by Smart Tiered Cache and Zone Cache Rules (`Origin Range Requests: On`, 30d–1y edge TTL, `immutable`).
4. **R2 Storage Tiering (`10 GB` free/month)**:
   - Incomplete multipart uploads abort after **24 hours (`86400s`)** instead of 7 days.
   - Cold archival buckets (`aphrody-backups`, `aphrody-chromium`, `aphrody-gnu`, `aphrody-lore`) are targeted for offload to self-hosted S3 (`Garage` on OVH `edge_proxy`), keeping active hot R2 assets near or below the 10 GB free threshold.
5. **Single-Owner Hostnames**: Wildcard Worker routes (`*.aphrody.app/*`) and dual Custom-Domain + Worker-Route bindings are flagged by `tools/reconcile.ts` for cutover so cached R2 GETs never trigger unnecessary Worker invocations.
