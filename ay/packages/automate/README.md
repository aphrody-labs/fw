# @aphrody/ay-automate

<!-- catalog:begin unit -->
<!-- prettier-ignore-start -->
> Workflow automation engine: definitions (zod schema), node catalog, expression language, executor with injected services, templates, n8n import/export. No outside access of its own.

| Type | Path |
| :-- | :-- |
| Bun package | `packages/automate` |

**Commands**

```sh
bun run --cwd packages/automate check
bun run --cwd packages/automate fmt
bun run --cwd packages/automate test
```

Part of Aphrody: [aphrody-labs/ay](../../README.md).
<!-- prettier-ignore-end -->
<!-- catalog:end -->

Workflow automation engine. Identifiers are French (`valider`, `executer`, `resoudre`).

- `Definition` (zod schema), `valider(def)`: static validation of a workflow.
- `MODELES` / `catalogueJson()`: bundled templates and the node catalog.
- `executer(def, services, input)`: runs a workflow; every outside access goes through the injected `Services`.
- `@aphrody/ay-automate/expressions`: expression language (`resoudre`, `FILTRES`).
- `importerN8n` / `estN8n`: n8n workflow import.

Tests live next to the sources (`bun test packages/automate`).
