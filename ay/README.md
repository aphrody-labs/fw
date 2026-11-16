# Aphrody

Aphrody crafts an OS, a browser, a Google and a Cloudflare/Vercel, each in whatever language does the job.

| Product | Replica of | Status |
|---|---|---|
| Aphrody OS | The Googlebook experience | see [features](docs/features.md#os) |
| Aphrody Browser | Chromium-based browsers | see [features](docs/features.md#browser) |
| Aphrody Search | google.com | see [features](docs/features.md#search) |
| Aphrody Cloud | Cloudflare Workers + Vercel | see [features](docs/features.md#cloud) |
| Craft Anything | storytold/craft-anything | see [features](docs/features.md#craft) |

- [Specification](docs/spec.md), [architecture](docs/architecture.md), [decisions](docs/adr/)
- [Feature matrix](docs/features.md): the scope, each row with its reference and its proof

```sh
bun run check      # feature matrix and tool tests
bun run features   # regenerate docs/features.md
```
