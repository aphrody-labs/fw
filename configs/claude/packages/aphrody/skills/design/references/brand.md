<!-- Merged from skill `brand` into `design`. Original frontmatter:
name: brand
description: Brand voice, visual identity, messaging frameworks, asset management, brand consistency. Activate for branded content, tone of voice, marketing assets, brand compliance, style guides.
argument-hint: "[update|review|create] [args]"
metadata:
  author: claudekit
  version: "1.0.0"
-->
# brand

> Brand voice, visual identity, messaging frameworks, asset management, brand consistency. Activate for branded content, tone of voice, marketing assets, brand compliance, style guides.


# Brand

Brand identity, voice, messaging, asset management, and consistency frameworks.

## When to Use

- Brand voice definition and content tone guidance
- Visual identity standards and style guide development
- Messaging framework creation
- Brand consistency review and audit
- Asset organization, naming, and approval
- Color palette management and typography specs

## Quick Start

**Inject brand context into prompts:**

```bash
node references/brand/scripts/inject-brand-context.cjs
node references/brand/scripts/inject-brand-context.cjs --json
```

**Validate an asset:**

```bash
node references/brand/scripts/validate-asset.cjs <asset-path>
```

**Extract/compare colors:**

```bash
node references/brand/scripts/extract-colors.cjs --palette
node references/brand/scripts/extract-colors.cjs <image-path>
```

## Brand Sync Workflow

```bash
# 1. Edit docs/brand-guidelines.md (or use /brand update)
# 2. Sync to design tokens
node references/brand/scripts/sync-brand-to-tokens.cjs
# 3. Verify
node references/brand/scripts/inject-brand-context.cjs --json | head -20
```

**Files synced:**

- `docs/brand-guidelines.md` → Source of truth
- `assets/design-tokens.json` → Token definitions
- `assets/design-tokens.css` → CSS variables

## Subcommands

| Subcommand | Description                                          | Reference              |
| ---------- | ---------------------------------------------------- | ---------------------- |
| `update`   | Update brand identity and sync to all design systems | `references/brand/references/update.md` |

## References

| Topic               | File                                      |
| ------------------- | ----------------------------------------- |
| Voice Framework     | `references/brand/references/voice-framework.md`           |
| Visual Identity     | `references/brand/references/visual-identity.md`           |
| Messaging           | `references/brand/references/messaging-framework.md`       |
| Consistency         | `references/brand/references/consistency-checklist.md`     |
| Guidelines Template | `references/brand/references/brand-guideline-template.md`  |
| Asset Organization  | `references/brand/references/asset-organization.md`        |
| Color Management    | `references/brand/references/color-palette-management.md`  |
| Typography          | `references/brand/references/typography-specifications.md` |
| Logo Usage          | `references/brand/references/logo-usage-rules.md`          |
| Approval Checklist  | `references/brand/references/approval-checklist.md`        |

## Scripts

| Script                             | Purpose                                           |
| ---------------------------------- | ------------------------------------------------- |
| `references/brand/scripts/inject-brand-context.cjs` | Extract brand context for prompt injection        |
| `references/brand/scripts/sync-brand-to-tokens.cjs` | Sync brand-guidelines.md → design-tokens.json/css |
| `references/brand/scripts/validate-asset.cjs`       | Validate asset naming, size, format               |
| `references/brand/scripts/extract-colors.cjs`       | Extract and compare colors against palette        |

## Templates

| Template                                | Purpose                                  |
| --------------------------------------- | ---------------------------------------- |
| `references/brand/templates/brand-guidelines-starter.md` | Complete starter template for new brands |

## Routing

1. Parse subcommand from `$ARGUMENTS` (first word)
2. Load corresponding `references/brand/references/{subcommand}.md`
3. Execute with remaining arguments
