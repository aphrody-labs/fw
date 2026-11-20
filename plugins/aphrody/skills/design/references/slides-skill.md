<!-- Merged from skill `slides` into `design`. Original frontmatter:
name: slides
description: Create strategic HTML presentations with Chart.js, design tokens, responsive layouts, copywriting formulas, and contextual slide strategies.
argument-hint: "[topic] [slide-count]"
metadata:
  author: claudekit
  version: "1.0.0"
-->
# slides

> Create strategic HTML presentations with Chart.js, design tokens, responsive layouts, copywriting formulas, and contextual slide strategies.


# Slides

Strategic HTML presentation design with data visualization.

## When to Use

- Marketing presentations and pitch decks
- Data-driven slides with Chart.js
- Strategic slide design with layout patterns
- Copywriting-optimized presentation content

## Subcommands

| Subcommand | Description                          | Reference              |
| ---------- | ------------------------------------ | ---------------------- |
| `create`   | Create strategic presentation slides | `references/slides-skill/references/create.md` |

## References (Knowledge Base)

| Topic                | File                                 |
| -------------------- | ------------------------------------ |
| Layout Patterns      | `references/slides-skill/references/layout-patterns.md`      |
| HTML Template        | `references/slides-skill/references/html-template.md`        |
| Copywriting Formulas | `references/slides-skill/references/copywriting-formulas.md` |
| Slide Strategies     | `references/slides-skill/references/slide-strategies.md`     |

## Routing

1. Parse subcommand from `$ARGUMENTS` (first word)
2. Load corresponding `references/slides-skill/references/{subcommand}.md`
3. Execute with remaining arguments
