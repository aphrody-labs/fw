# Organization standard

This repository follows the **Aphrody Labs standard**, version 1.0.0 (2026-11-17), kept in one place so it is never
copied: [`aphrody-labs/.github`](https://github.com/aphrody-labs/.github).

| Document | What it defines |
|---|---|
| [STANDARD.md](https://github.com/aphrody-labs/.github/blob/main/STANDARD.md) | Repository classes, layout, README/Markdown/MDX rules, naming, labels, issue forms, rulesets, security, Actions, release and publishing, Cloudflare, profile |
| [standard.toml](https://github.com/aphrody-labs/.github/blob/main/standard.toml) | The machine-readable twin: labels, required files, settings, ruleset JSON, pinned actions, Cloudflare policy, frozen domains |
| [templates/README.template.md](https://github.com/aphrody-labs/.github/blob/main/templates/README.template.md) | README template: title, one-sentence description, at most five badges, then Why, Install, Usage, Documentation, Development, License |
| [templates/doc.template.mdx](https://github.com/aphrody-labs/.github/blob/main/templates/doc.template.mdx) | MDX page template: `title` and `description` front matter, one H1 that equals the title, few typed components |

The repository is the SDK, so it is an **original** in the standard's classes and gets everything the standard asks of an
original. How it applies the parts that touch design and documentation:

- **Docs**: README, this file and the ADRs are GitHub-flavored Markdown; published pages are MDX under `docs/**/*.mdx`
  with the template above. Headings are sentence case, English first, other languages from the i18n catalogs.
- **Design**: the README and MDX templates, the label palette and the design-system tokens in use are drawn in the
  Figma page "Standard" (`docs/mirrors.toml` [figma]); code and Figma use the same Material 3 variables.
- **Checks**: `bun run check` runs the feature-matrix, path and test gates; the reusable `ci-bun` workflow of the
  standard (required check `ci / gate`) wraps them when the apply phase wires this repository.
- **Rulesets**: `main-integrity` and `tags-immutable` on every original; `main-gated` (pull request, required check) only
  for repositories that opt in. Agents commit to `main` by path.
- **Frozen**: `aphrody.com` and `aphrody.app` belong to the aphrody monorepo; this repository records them and never
  changes their DNS, zones or routes (`ay.toml` [domains]).

Deviations from the standard are written here with their reason. Today: none.
