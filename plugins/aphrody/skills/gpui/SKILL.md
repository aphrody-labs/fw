---
name: gpui
description: "Native UI in Rust with GPUI: core framework (entities, render, elements, Taffy layout, focus, actions, backends). Use before designing or editing native UI, windows, events or GPUI crate internals."
---
> GPUI Kit apps and their design guides, GPUI Fast retained views, gpui::test and Criterion benchmarks.

# GPUI: native UI in Rust

`references/core.md` is the former body of this skill (core GPUI): read it first, then the reference for the layer you touch.

Read the reference that matches the task in full before acting. Paths written inside reference files are relative to this skill's directory (the one holding this SKILL.md). Where a reference names another merged skill, open `references/<name>.md`; its helper files are under `references/<name>/`.

| Reference | Use when |
| --- | --- |
| [references/core.md](references/core.md) | Former body of this skill (read first) |
| [references/gpui-brain.md](references/gpui-brain.md) | Master router and offline knowledge index for GPUI Core (crates/gpui), GPUI Kit 0.10 (gpui-base / gpui-component / gpui-kit), GPUI Fast 0.1.17 (retained views, splicing, layout keys, text replay, GPU tile scroll layers, native window composition), and Aphrody's unification matrix. Use before building, porting,... |
| [references/gpui-kit.md](references/gpui-kit.md) | How to build desktop applications with GPUI Kit, the Rust framework published as the gpui-kit crate (GPUI plus gpui_kit::component, gpui_kit::base, gpui_kit::assets). Use when setting up a gpui-kit app, choosing or using a component (Button, Input, Select, Dialog, Sheet, Tabs, Sidebar, List, DataTable, Tree, Chart,... |
| [references/gpui-kit-design-guides.md](references/gpui-kit-design-guides.md) | The normative Design Guides for GPUI Kit desktop applications. Read in full before designing or changing any screen, layout, spacing, visual hierarchy, color, density, component choice, interaction state, overlay, motion, data-heavy view, or interface copy in a GPUI Kit (gpui-kit / gpui-component) application, and... |
| [references/gpui-fast.md](references/gpui-fast.md) | Master engineering reference for GPUI Fast (longbridge/gpui-fast) retained-mode frame reuse, view splicing, dependency tracking, persistent Taffy layout keys, text measurement Carry/Replay, GPU tile scroll layers, dynamic GPU textures, and native window composition in GPUI / GPUI Kit / Aphrody. |
| [references/gpui-test.md](references/gpui-test.md) | Use when writing, debugging, or reproducing GPUI tests in Zed, including gpui::test arguments, TestAppContext parameters, scheduler seeds, ITERATIONS/SEED reproduction, parking failures, and pending task traces. |
| [references/gpui-bench.md](references/gpui-bench.md) | Design, write, review, run, and interpret production-shaped GPUI Criterion benchmarks, including gpui::bench, BenchAppContext, renderer and task benchmarks, headless Metal frame data, responsiveness and hang regressions, feature isolation from test-support, and before/after performance evidence. |

<!-- merge-skill-families:v1 -->
