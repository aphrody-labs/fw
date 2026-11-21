---
name: replica
description: "Clone an existing app end to end: recon map, stack and schema, design tokens, screen-by-screen build, backend, tests, parity diff. Use when the user says replicate, clone, rebuild or copy an app"
---
> user-review research, rebrand, launch copy and deploy. , or names a stage (recon, architect, design, build, backend, test, diff, entrepreneur, brand, launch, deploy).

# Replica: clone an app end to end

Replica is a pipeline. Run the stages in this order and pass each stage's output file to the next: recon, architect, design, build, backend, test, diff, entrepreneur, brand, launch, deploy.

Read the reference that matches the task in full before acting. Paths written inside reference files are relative to this skill's directory (the one holding this SKILL.md). Where a reference names another merged skill, open `references/<name>.md`; its helper files are under `references/<name>/`.

- A reference that says `/replica-<stage>` means: open `references/replica-<stage>.md`.
- The helper scripts and data files of a stage live under `references/replica-<stage>/`.

| Reference | Use when |
| --- | --- |
| [references/core.md](references/core.md) | Former body of this skill (read first) |
| [references/replica-recon.md](references/replica-recon.md) | Reverse-engineers any app into a recon map: screen inventory, user flows, component list, inferred data model and a feature matrix, from public pages, screenshots, app store listings, help docs and the user's own account. The first step of the Replica pack. Use when the user says "clone this app", "reverse engineer... |
| [references/replica-architect.md](references/replica-architect.md) | Plans the stack, database schema and API for an app clone, from the recon map replica-recon wrote. Picks boring, managed tech, turns the inferred data model into real SQL with indexes and access rules, lists every route by flow, and orders the build as a thin vertical slice first. Use when the user says "plan the... |
| [references/replica-design.md](references/replica-design.md) | Rebuilds an app's design system for a clone: colour roles, type scale, spacing, radius, shadows and every component with its states, as design tokens plus component specs, with original assets instead of the target's logos, icons, illustrations or licensed fonts. Includes a WCAG contrast checker. Use when the user... |
| [references/replica-build.md](references/replica-build.md) | Rebuilds an app screen by screen from the recon map: app shell first, then the core flow as a vertical slice, then every screen with all its states, ticking off the feature matrix as it goes. Writes every line fresh, never the original's code, assets or copy. Use when the user says "build the clone", "build screen... |
| [references/replica-backend.md](references/replica-backend.md) | Builds the backend of an app clone: auth, database migrations and access rules, payments with Stripe, email, background jobs and third-party integrations through official APIs only, plus a security checklist. Use when the user says "add login", "set up auth", "wire up the database", "add payments", "connect Stripe",... |
| [references/replica-test.md](references/replica-test.md) | Clicks through every flow of an app clone and tests it for bugs: a test plan generated from the recon flows with happy paths and edge cases, Playwright end-to-end tests where possible, a browser click-through where not, and bug reports in a fixed format with severity, steps and evidence. Use when the user says "test... |
| [references/replica-diff.md](references/replica-diff.md) | Compares an app clone against the original: a feature parity score from the feature matrix (weighted by must, should, could) with the missing list in build order, plus a screenshot layout diff that ignores colour so a rebrand does not count against you. Two standard-library Python tools. Use when the user says "how... |
| [references/replica-entrepreneur.md](references/replica-entrepreneur.md) | Researches the app being cloned and reads what its real users say in public reviews (App Store, Google Play, G2, Capterra, Trustpilot, Reddit, Hacker News, its own feature-request board), then ranks what they hate, what is missing and what is unsolved, turns it into a fix plan for the clone and a positioning angle you... |
| [references/replica-brand.md](references/replica-brand.md) | Names and rebrands an app clone so it is the user's own: name candidates with the trademark, domain, store and handle checks to run, a new palette checked for contrast, a logo brief, a voice guide, and a sweep tool that finds anything of the original left in the codebase. Use when the user says "name my app", "rebrand... |
| [references/replica-launch.md](references/replica-launch.md) | Writes the launch for an app clone: the landing page built on the positioning angle, pricing set against the original's public pricing and its users' complaints, and the App Store and Google Play listing linted against the stores' limits and copycat rules. Use when the user says "landing page", "pricing", "how much... |
| [references/replica-deploy.md](references/replica-deploy.md) | Ships an app clone live on the user's own domain: a preflight gate (tests green, parity must-haves done, rebrand sweep clean, listing linted, legal pages up), production database and env vars, the host, DNS records for the domain and for email, Stripe live mode, OAuth redirects, monitoring, and mobile builds to... |

<!-- merge-skill-families:v1 -->
