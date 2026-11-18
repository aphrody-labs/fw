/**
 * Unified command registry for the `ay` CLI.
 *
 * Maps every standalone `tools/*.ts` script, app entrypoint, and built-in
 * subcommand into one Bun-native dispatch table.
 */

export type CommandKind = "tool" | "app" | "builtin";

export type CommandCategory =
  | "core"
  | "runtime"
  | "cloud"
  | "infra"
  | "quality"
  | "data"
  | "meta";

export interface CommandEntry {
  readonly name: string;
  readonly aliases?: readonly string[];
  readonly category: CommandCategory;
  readonly kind: CommandKind;
  readonly target: string;
  readonly summary: string;
}

export const COMMANDS: readonly CommandEntry[] = [
  // Core & Apps
  {
    name: "completions",
    aliases: ["comp"],
    category: "meta",
    kind: "builtin",
    target: "builtin:completions",
    summary: "Generate shell completions (bash, zsh, fish, nu, pwsh)",
  },
  {
    name: "compile",
    aliases: ["aot"],
    category: "meta",
    kind: "builtin",
    target: "builtin:compile",
    summary: "Compile `ay` into a standalone native binary via `bun build --compile`",
  },
  {
    name: "doctor",
    aliases: ["health"],
    category: "meta",
    kind: "builtin",
    target: "builtin:doctor",
    summary: "Verify Bun runtime, workspace root, and CLI registry integrity",
  },
  {
    name: "wck",
    aliases: ["mwg", "web-check"],
    category: "quality",
    kind: "builtin",
    target: "builtin:wck",
    summary: "Modern Web Guidance, Core Web Vitals, SEO/A11y & legacy-scan gate",
  },

  // Runtime & Corelib
  {
    name: "abi",
    category: "runtime",
    kind: "tool",
    target: "tools/abi.ts",
    summary: "ABI surface and C/JNI header verification",
  },
  {
    name: "corelib",
    category: "runtime",
    kind: "tool",
    target: "tools/corelib.ts",
    summary: "Corelib build, test, and component orchestration",
  },
  {
    name: "corelib-component",
    category: "runtime",
    kind: "tool",
    target: "tools/corelib-component.ts",
    summary: "Inspect and manage corelib components",
  },
  {
    name: "corelib-qemu",
    aliases: ["qemu"],
    category: "runtime",
    kind: "tool",
    target: "tools/corelib-qemu.ts",
    summary: "Run corelib verification inside QEMU",
  },
  {
    name: "linux-aphrody",
    category: "runtime",
    kind: "tool",
    target: "tools/linux-aphrody.ts",
    summary: "Linux kernel / Aphrody OS integration tooling",
  },
  {
    name: "linux-docs",
    category: "runtime",
    kind: "tool",
    target: "tools/linux-docs.ts",
    summary: "Linux documentation indexing and lookup",
  },
  {
    name: "linux-sources",
    category: "runtime",
    kind: "tool",
    target: "tools/linux-sources.ts",
    summary: "Linux source tree management and verification",
  },
  {
    name: "shells",
    category: "runtime",
    kind: "tool",
    target: "tools/shells.ts",
    summary: "Cross-platform shell environment diagnostics",
  },
  {
    name: "ts",
    category: "runtime",
    kind: "tool",
    target: "tools/ts.ts",
    summary: "TypeScript / Bun typecheck and workspace runner",
  },

  // Cloud & Edge
  {
    name: "cf-quota",
    aliases: ["quota"],
    category: "cloud",
    kind: "tool",
    target: "tools/cf-quota.ts",
    summary: "Cloudflare Workers/R2/D1/KV quota guard and telemetry",
  },
  {
    name: "cloud-map",
    category: "cloud",
    kind: "tool",
    target: "tools/cloud-map.ts",
    summary: "Cloudflare & edge topology map",
  },
  {
    name: "deploy",
    category: "cloud",
    kind: "tool",
    target: "tools/deploy.ts",
    summary: "Deploy Aphrody services across Cloudflare and fleet VPS",
  },
  {
    name: "r2-cdn-sync",
    aliases: ["cdn-sync", "r2-sync"],
    category: "cloud",
    kind: "tool",
    target: "tools/r2-cdn-sync.ts",
    summary: "Sync aphrody-cdn R2 bucket via Bun.s3 to CloudFront origin",
  },
  {
    name: "serve-closed-loop",
    category: "cloud",
    kind: "tool",
    target: "tools/serve-closed-loop.ts",
    summary: "Closed-loop local/edge verification server",
  },

  // Infra & Fleet
  {
    name: "disks",
    category: "infra",
    kind: "tool",
    target: "tools/disks.ts",
    summary: "Disk and storage layout inspection",
  },
  {
    name: "hosts",
    category: "infra",
    kind: "tool",
    target: "tools/hosts.ts",
    summary: "Fleet host inventory and SSH reachability",
  },
  {
    name: "local",
    category: "infra",
    kind: "tool",
    target: "tools/local.ts",
    summary: "Local workstation environment setup and checks",
  },
  {
    name: "org-inventory",
    aliases: ["org"],
    category: "infra",
    kind: "tool",
    target: "tools/org-inventory.ts",
    summary: "GitHub org repository and package inventory",
  },
  {
    name: "recon",
    category: "infra",
    kind: "tool",
    target: "tools/recon.ts",
    summary: "Infrastructure and repo reconnaissance",
  },
  {
    name: "recover",
    category: "infra",
    kind: "tool",
    target: "tools/recover.ts",
    summary: "Recovery and repair workflows",
  },
  {
    name: "submodules",
    category: "infra",
    kind: "tool",
    target: "tools/submodules.ts",
    summary: "Submodule and vendor manifest synchronization",
  },
  {
    name: "sync-host",
    category: "infra",
    kind: "tool",
    target: "tools/sync-host.ts",
    summary: "Synchronize workspace state to fleet hosts",
  },

  // Quality & Hygiene
  {
    name: "bun-native",
    aliases: ["native"],
    category: "quality",
    kind: "tool",
    target: "tools/bun-native.ts",
    summary: "Enforce Bun-native APIs and zero Node/legacy regressions",
  },
  {
    name: "claims",
    category: "quality",
    kind: "tool",
    target: "tools/claims.ts",
    summary: "Verify architectural claims and invariants",
  },
  {
    name: "dedup",
    category: "quality",
    kind: "tool",
    target: "tools/dedup.ts",
    summary: "Detect duplicate files, symbols, and packages",
  },
  {
    name: "vendor-dedup",
    aliases: ["vdedup", "dup-vendor"],
    category: "quality",
    kind: "tool",
    target: "tools/vendor-dedup.ts",
    summary: "Find duplicate vendor and third_party trees across workspace checkouts with eza, rg, and fuzzy matching",
  },
  {
    name: "code-dedup",
    aliases: ["cdedup", "dup-code"],
    category: "quality",
    kind: "tool",
    target: "tools/code-dedup.ts",
    summary: "Detect code clones, code-graph overlaps, and upstream repos copied into first-party via git, tree-sitter, difft, rg, eza, and fzf",
  },
  {
    name: "legacy",
    category: "quality",
    kind: "tool",
    target: "tools/legacy.ts",
    summary: "Scan and audit legacy prototypes or deprecated patterns",
  },
  {
    name: "paths",
    category: "quality",
    kind: "tool",
    target: "tools/paths.ts",
    summary: "Enforce relative paths and zero hardcoded machine paths",
  },
  {
    name: "private-gate",
    aliases: ["privacy"],
    category: "quality",
    kind: "tool",
    target: "tools/private-gate.ts",
    summary: "Prevent secrets and private paths from leaking into git",
  },

  // Data, Planning & Research
  {
    name: "absorb",
    category: "data",
    kind: "tool",
    target: "tools/absorb.ts",
    summary: "Absorb external crates/repos into the unified workspace",
  },
  {
    name: "claude-sessions",
    category: "data",
    kind: "tool",
    target: "tools/claude-sessions.ts",
    summary: "Inspect and export Claude session transcripts",
  },
  {
    name: "features",
    category: "data",
    kind: "tool",
    target: "tools/features.ts",
    summary: "Feature matrix and parity tracking",
  },
  {
    name: "listings",
    category: "data",
    kind: "tool",
    target: "tools/listings.ts",
    summary: "Generate structured workspace listings",
  },
  {
    name: "plan",
    category: "data",
    kind: "tool",
    target: "tools/plan.ts",
    summary: "Execution plan tracking and verification",
  },
  {
    name: "rust-map",
    category: "data",
    kind: "tool",
    target: "tools/rust-map.ts",
    summary: "Rust workspace crate dependency and symbol map",
  },
  {
    name: "science",
    category: "data",
    kind: "tool",
    target: "tools/science.ts",
    summary: "Benchmarks, telemetry, and experimental analysis",
  },
];

export function resolveCommand(name: string): CommandEntry | undefined {
  const normalized = name.trim().toLowerCase();
  return COMMANDS.find(
    (cmd) =>
      cmd.name === normalized ||
      cmd.aliases?.some((alias) => alias === normalized),
  );
}
