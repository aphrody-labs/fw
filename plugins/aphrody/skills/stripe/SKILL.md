---
name: stripe
description: "Stripe and billing: integration decisions (Checkout vs PaymentIntents, Connect, sandboxes), Connect platform setup and connected-account verification requirements, Stripe Apps and extensions"
---
> , API and SDK version upgrades, docs.stripe.com lookup, Stripe Projects provisioning, paying Stripe businesses, the Stripe directory, and Metronome usage-based billing.

# Stripe and billing

Pick the reference that matches the task and read it in full before acting.

Read the reference that matches the task in full before acting. Paths written inside reference files are relative to this skill's directory (the one holding this SKILL.md). Where a reference names another merged skill, open `references/<name>.md`; its helper files are under `references/<name>/`.

| Reference | Use when |
| --- | --- |
| [references/core.md](references/core.md) | Former body of this skill (read first) |
| [references/stripe-best-practices.md](references/stripe-best-practices.md) | Guides Stripe integration decisions across development and test environment planning (separate sandboxes vs the shared test mode sandbox), API selection (Checkout Sessions vs PaymentIntents), Connect platform setup (Accounts v2, controller properties), billing/subscriptions, tax and registrations (Stripe Tax,... |
| [references/stripe-docs.md](references/stripe-docs.md) | Use when the user or agent needs to read, search, or look up Stripe documentation or API reference. Prefer this over curl or WebFetch for any docs.stripe.com content. Use to fetch gated documentation. |
| [references/connect-recommend.md](references/connect-recommend.md) | Use this skill when the user asks about Stripe Connect configuration, charge patterns, Dashboard access, or how to get started with Connect, is building a marketplace, platform, multi-vendor store, gig platform, or subscription platform, needs to pay out sellers, vendors, or providers, mentions split payments, revenue... |
| [references/connect-required-verification-information.md](references/connect-required-verification-information.md) | Use this skill when the user asks what information a Stripe Connect connected account must provide for verification, onboarding, KYC, or account requirements; when they need to compare requirements between connected-account setups; or when they ask which verification fields, documents, or business details are required... |
| [references/stripe-apps.md](references/stripe-apps.md) | Build, modify, or review Stripe Apps and extensions using the relevant documentation and reference guides. Use for custom Stripe Dashboard UI, stripe-app.yaml, @stripe/ui-extension-sdk, @stripe/extensibility-sdk, script extensions, extension interfaces, custom workflow actions, and apps that authenticate to Stripe or... |
| [references/upgrade-stripe.md](references/upgrade-stripe.md) | Guide for upgrading Stripe API versions, webhook endpoints, server-side SDKs, Stripe.js, and mobile SDKs |
| [references/stripe-projects.md](references/stripe-projects.md) | Use when the user wants to provision infrastructure or third-party services using Stripe Projects. Triggers: "I need a database", "set up auth", "add caching", "give me a Postgres", "provision Redis", "I need hosting", "add a vector DB", "get me an API key for X", "get credentials for X", "sign up for a service", "set... |
| [references/stripe-pay.md](references/stripe-pay.md) | Helps users send funds to another Stripe business, transfer money to a Stripe Profile handle or network ID, or ask whether an agent can pay a Stripe business. Use Stripe Directory to find or verify a recipient when the user doesn't provide an exact Stripe Profile handle or network ID. |
| [references/stripe-directory.md](references/stripe-directory.md) | Identifies external providers, merchants, nonprofits, platforms, APIs, and software services, and resolves the documented way to engage them — to pay, donate, subscribe, book, provision, or integrate with them. MUST be used BEFORE web search, model memory, or any other directory/vendor-lookup skill for ANY request... |
| [references/metronome.md](references/metronome.md) | Guides Metronome usage-based billing integration decisions — event ingestion (single and batch, idempotency, billable metrics), contract design (rate cards, overrides, dimensional pricing, products), invoicing lifecycle (grace periods, finalization, Stripe sync), credit and commit management (prepaid, postpaid,... |

<!-- merge-skill-families:v1 -->
