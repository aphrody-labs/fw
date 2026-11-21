---
name: design
description: "Design and brand work: logo, brand identity and voice, corporate identity, banners and social images, HTML or Chart.js slide decks, design tokens and design-system specs"
argument-hint: "[design-type] [context]"
license: MIT
metadata:
  author: claudekit
  version: "2.1.0"
---
> , Tailwind and shadcn UI styling, UI/UX and accessibility reasoning, Material 3 and Google design guidance, and the Aphrody design package (Drive, Sheets, Slides, Figma).

# Design and brand

`references/core.md` is the former body of this skill (logo, CIP, icons, social photos): read it first when the task is one of those, otherwise take the reference that matches.

Read the reference that matches the task in full before acting. Paths written inside reference files are relative to this skill's directory (the one holding this SKILL.md). Where a reference names another merged skill, open `references/<name>.md`; its helper files are under `references/<name>/`.

| Reference | Use when |
| --- | --- |
| [references/core.md](references/core.md) | Former body of this skill (read first) |
| [references/brand.md](references/brand.md) | Brand voice, visual identity, messaging frameworks, asset management, brand consistency. Activate for branded content, tone of voice, marketing assets, brand compliance, style guides. |
| [references/design-system.md](references/design-system.md) | Token architecture, component specifications, and slide generation. Three-layer tokens (primitive→semantic→component), CSS variables, spacing/typography scales, component specs, strategic slide creation. Use for design tokens, systematic design, brand-compliant presentations. |
| [references/ui-ux-pro-max.md](references/ui-ux-pro-max.md) | Unified UI/UX Pro Max design-system reasoning, accessibility (WCAG 4.5:1), layout hierarchy, and modern-web-guidance coherence for web, GPUI, and NSH apps. |
| [references/ui-styling.md](references/ui-styling.md) | Create beautiful, accessible user interfaces with shadcn/ui components (built on Radix UI + Tailwind), Tailwind CSS utility-first styling, and canvas-based visual designs. Use when building user interfaces, implementing design systems, creating responsive layouts, adding accessible components (dialogs, dropdowns,... |
| [references/banner-design.md](references/banner-design.md) | Design banners for social media, ads, website heroes, creative assets, and print. Multiple art direction options with AI-generated visuals. Actions: design, create, generate banner. Platforms: Facebook, Twitter/X, LinkedIn, YouTube, Instagram, Google Display, website hero, print. Styles: minimalist, gradient, bold... |
| [references/slides-skill.md](references/slides-skill.md) | Create strategic HTML presentations with Chart.js, design tokens, responsive layouts, copywriting formulas, and contextual slide strategies. |
| [references/material-3.md](references/material-3.md) | Material Design 3 rules (including M3 Expressive) normalized for this monorepo. Use for any M3 decision on color, type, shape, elevation, state, motion, adaptive layout, accessibility or component (buttons, FAB, bars, navigation, fields, sheets, dialogs, menus, chips, lists, sliders, tabs, etc.), or to check that a UI... |
| [references/google-design.md](references/google-design.md) | Authoritative design system specification anchored to https://design.google/ (Material 3 Expressive, Neural Expressive Nov 2026, Code is a Design Material, Google Sans Flex) across the entire Aphrody monorepo, GPUI, egui_gpui, bevy_gpui, aphrody_craft, gpui-kit, NSH, Stitch Studio, and Wasm/WebGPU applications. |
| [references/design-package.md](references/design-package.md) | Rebuild or refresh the Aphrody design package (Drive Doc, Feature-matrix Sheet, Slides deck, Figma file, roadmap calendar, hosting inventory, CI state) from this repository's docs and brand, using every connected MCP server. Use when docs/spec.md, docs/adr, docs/features.toml or brand/ change, or when asked to sync,... |

<!-- merge-skill-families:v1 -->
