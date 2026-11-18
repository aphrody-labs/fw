# Vercel Agent Skills Registry

`ay-vercel` aggregates official skills from `vercel/vercel-plugin`, `vercel-labs/agent-skills`, `vercel-labs/skills`, `vercel/next.js`, `vercel/ai`, `vercel/turborepo`, `vercel/sandbox`, `vercel/workflow`, `vercel/flags`, `vercel/chat`, `vercel/eve`, `vercel/components.build`, `vercel/marketplace-skills`, `vercel-labs/next-maintainer-skills`, `vercel-labs/migration-skills`, `vercel-labs/vercel-kb-skills`, `vercel-labs/json-render`, `vercel-labs/agent-browser`, `vercel-labs/agent-desktop`, and companion `vercel-labs/*` repositories.

- Discovery RFC index: [`.well-known/agent-skills/index.json`](../.well-known/agent-skills/index.json)
- Provenance registry: [`skills/REGISTRY.json`](../skills/REGISTRY.json)

Sync and verify SHA-256 digests:

```sh
just skills-sync
bun tools/skills-sync.ts --check
```
