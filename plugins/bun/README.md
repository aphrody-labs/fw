# bun

Cross-CLI plugin for Bun-native engineering workflows across Claude Code, Antigravity (`agy`), and Gemini CLI.

## Features

- **Transparent Command Rewriting**: automatically routes legacy `npm`, `npx`, `yarn`, `pnpm`, `node`, `tsc`, `prettier`, `eslint`, `oxlint`, and `oxfmt` invocations to native `bun`, `bunx`, `bun check`, `bun lint`, and `bun fmt` without blocking or permission prompts.
- **Checkout Awareness**: uses `bun bd test` cleanly when executing inside a Bun source checkout.
- **36 Specialized Skills**: covers `Bun.*` native APIs, bundler, test runner, Rust internals, JSC/WebKit C++ bindings, Win32, Linux kernel helpers, and Node-to-Bun migration.

## Install

```text
claude plugin marketplace add C:\src\fw
claude plugin install bun@fw
bun ~/.gemini/scripts/install.ts   # agy
```

## Privacy

This plugin runs strictly locally inside your terminal session, collects zero analytics or telemetry, and transmits no workspace files or environment secrets externally.
