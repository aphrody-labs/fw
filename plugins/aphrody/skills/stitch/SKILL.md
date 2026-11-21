---
name: stitch
description: "Google Stitch design workflow: generate and edit screens, DESIGN.md and SITE.md, design systems, code to design, design to React, React Native, shadcn and Remotion video. Use for any Stitch MCP task."
---
# Google Stitch

Stitch turns prompts and images into UI screens through the `stitch` MCP server. Pick the reference for the step.

Read the reference that matches the task in full before acting. Paths written inside reference files are relative to this skill's directory (the one holding this SKILL.md). Where a reference names another merged skill, open `references/<name>.md`; its helper files are under `references/<name>/`.

| Reference | Use when |
| --- | --- |
| [references/enhance-prompt.md](references/enhance-prompt.md) | Transforms vague UI ideas into polished, Stitch-optimized prompts. Enhances specificity, adds UI/UX keywords, injects design system context, and structures output for better generation results. |
| [references/generate-design.md](references/generate-design.md) | Generate new screens from text prompts or images, edit existing screens with prompts and design system tokens, and generate design variants using Stitch MCP. Includes prompt enhancement pipeline |
| [references/manage-design-system.md](references/manage-design-system.md) | Manage design systems in Stitch using MCP tools. Includes retrieval of assets, creating/updating design systems in Stitch, and applying them to screens. |
| [references/upload-to-stitch.md](references/upload-to-stitch.md) | Upload local assets (images, mockups, extracted HTML, design markdown) to a Stitch project. ALWAYS Use when you need to upload visual assets, HTML pages, or design docs to Stitch |
| [references/design-md.md](references/design-md.md) | Analyze Stitch projects and synthesize a semantic design system into DESIGN.md files |
| [references/extract-design-md.md](references/extract-design-md.md) | Extract a comprehensive design system (DESIGN.md) directly from frontend source code — React, Vue, Svelte. Use whenever the user wants to reverse-engineer a design system from an existing codebase |
| [references/extract-static-html.md](references/extract-static-html.md) | Extract self-contained static HTML from a built web application or React components by inlining CSS and images. Use whenever you need to capture a specific UI state, share a static version of a page |
| [references/code-to-design.md](references/code-to-design.md) | Convert frontend code (Vite, React, Angular, Vue, etc.) to a Stitch Design by chaining static HTML extraction, design system extraction |
| [references/site-md.md](references/site-md.md) | Analyze a project description or site requirements and synthesize a project constitution into SITE.md for the Stitch Build Loop |
| [references/stitch-loop.md](references/stitch-loop.md) | Teaches agents to iteratively build websites using Stitch with an autonomous baton-passing loop pattern |
| [references/taste-design.md](references/taste-design.md) | Semantic Design System Skill for Google Stitch. Generates agent-friendly DESIGN.md files that enforce premium, anti-generic UI standards — strict typography, calibrated color, asymmetric layouts |
| [references/react-components.md](references/react-components.md) | Converts Stitch designs into modular Vite and React components, or syncs/updates existing React components to align with the latest Stitch designs |
| [references/react-vite-dashboard.md](references/react-vite-dashboard.md) | Convert Stitch designs into production React + Vite dashboards with TanStack Query, accessible tokens from DESIGN.md, and Web3-ready patterns (ethers/viem). |
| [references/react-native.md](references/react-native.md) | Convert Stitch HTML designs to React Native components, or syncs/updates existing native components to align with the latest Stitch designs, using StyleSheet. |
| [references/shadcn-ui.md](references/shadcn-ui.md) | Expert guidance for integrating and building applications with shadcn/ui components, including component discovery, installation, customization, and best practices. |
| [references/remotion.md](references/remotion.md) | Generate walkthrough videos from Stitch projects using Remotion with smooth transitions, zooming, and text overlays |

<!-- merge-skill-families:v1 -->
