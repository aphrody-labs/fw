# Lot: Next.js features with no Rust equivalent, and the cost gap

Measured on prd (Bun 1.4.5 runs `next start`; load 27 to 36), same 100-row table, 3 runs of 6 s, 32 connections: Next.js 16.4.6 `/ssr` costs 5,629 CPU microseconds per request (142 requests per second, 282 MiB RSS), Leptos 0.8.23 SSR 30.2 (9,154 rps, 5.1 MiB), Dioxus 0.7.10 SSR 100.2 (4,025 rps, 5.3 MiB). Different frameworks do different work (Next runs React and the App Router); the numbers size the opportunity, they do not rank products.

Missing in Rust (matrix rows): server components, file-system routing convention, image optimisation parity (Dioxus manganis partial; `leptos_image` last updated 2024-02-05).

Decision recorded in the matrix: dragonballfr.com (Shenron) stays on Next.js (pinned `16.5.0-canary.5`) until a Leptos or Dioxus prototype of one Shenron route matches it on SSR output, i18n, images and auth; prototype lot: render the Shenron article route with Leptos on axum behind `ay-cf`.
