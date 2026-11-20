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
    name: "upgrade",
    aliases: ["update"],
    category: "meta",
    kind: "builtin",
    target: "builtin:upgrade",
    summary: "Fast-forward this checkout, install the locked dependencies and verify the registry (--check only reports)",
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
    name: "edge-config",
    category: "cloud",
    kind: "tool",
    target: "tools/edge-config.ts",
    summary: "Render apps/edge/wrangler.jsonc from apps/edge/edge.toml (routes, buckets, service bindings), or check it is current",
  },
  {
    name: "hygiene",
    category: "infra",
    kind: "tool",
    target: "tools/host-hygiene.ts",
    summary: "Enforce the disk policy of ay.toml [hygiene] on every host: targets, node_modules, caches, toolchains, containers, nginx logs, journal, undeclared units",
  },
  {
    name: "repo-archive",
    category: "infra",
    kind: "tool",
    target: "tools/repo-archive.ts",
    summary: "Archive satellite repositories from a reviewed ledger, with a dry run and an API check of the result",
  },
  {
    name: "edge-cutover",
    category: "cloud",
    kind: "tool",
    target: "tools/edge-cutover.ts",
    summary: "Plan, apply and roll back the move of live routes and custom domains to the unified edge Worker",
  },
  {
    name: "edge-wasm",
    category: "cloud",
    kind: "tool",
    target: "tools/edge-wasm.ts",
    summary: "Build and package the wasm32 modules the edge Worker bundles and serves",
  },
  {
    name: "release-publish",
    category: "cloud",
    kind: "tool",
    target: "tools/release-publish.ts",
    summary: "Publish a release (assets, SHA256SUMS, manifests) into the releases bucket layout",
  },
  {
    name: "r2-sync",
    aliases: ["cdn-sync", "r2-cdn-sync"],
    category: "cloud",
    kind: "tool",
    target: "tools/r2-sync.ts",
    summary: "Sync brand files, content-addressed assets and upstream mirrors into the R2 buckets of apps/edge/edge.toml",
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
  {
    name: "auto-build-aot",
    category: "cloud",
    kind: "tool",
    target: "tools/auto-build-aot.ts",
    summary: "Build the standalone Bun AOT executable of the Auto server",
  },
  {
    name: "auto-build-native",
    category: "cloud",
    kind: "tool",
    target: "tools/auto-build-native.ts",
    summary: "Build the native Auto units",
  },
  {
    name: "auto-cargo",
    category: "quality",
    kind: "tool",
    target: "tools/auto-cargo.ts",
    summary: "Run cargo for the Auto units on the toolchain they need",
  },
  {
    name: "auto-deploy-cf",
    category: "cloud",
    kind: "tool",
    target: "tools/auto-deploy-cf.ts",
    summary: "Deploy the Auto Workers to Cloudflare from declarative config, no literal ids or secrets",
  },
  {
    name: "auto-deploy-fleet",
    category: "infra",
    kind: "tool",
    target: "tools/auto-deploy-fleet.ts",
    summary: "Deploy the self-hosted Auto server to the test and production hosts of ay.toml [hosts]",
  },
  {
    name: "auto-openapi",
    category: "data",
    kind: "tool",
    target: "tools/auto-openapi.ts",
    summary: "Write docs/auto/openapi.json from the Auto server OpenAPI builder",
  },
  {
    name: "auto-validate-facturx",
    category: "quality",
    kind: "tool",
    target: "tools/auto-validate-facturx.ts",
    summary: "Validate Factur-X CII XML against the EN 16931 XSD and Schematron",
  },
  {
    name: "cf-audit",
    category: "quality",
    kind: "tool",
    target: "tools/cf-audit.ts",
    summary: "Offline audit of cf.toml, the R2 catalog and the wrangler configs",
  },
  {
    name: "cf-bench-http",
    category: "quality",
    kind: "tool",
    target: "tools/cf-bench-http.ts",
    summary: "Benchmark HTTP clients against live Cloudflare endpoints",
  },
  {
    name: "cf-bootstrap-local",
    category: "cloud",
    kind: "tool",
    target: "tools/cf-bootstrap-local.ts",
    summary: "Generate the git-ignored cf.local.toml from the live Cloudflare API",
  },
  {
    name: "cf-cluster-proxy",
    category: "infra",
    kind: "tool",
    target: "tools/cf-cluster-proxy.ts",
    summary: "Render the edge proxy, origin firewall, VPN routes and cloudflared config",
  },
  {
    name: "cf-finops",
    category: "cloud",
    kind: "tool",
    target: "tools/cf-finops.ts",
    summary: "Report live Workers and R2 costs and savings",
  },
  {
    name: "cf-http-scan",
    category: "quality",
    kind: "tool",
    target: "tools/cf-http-scan.ts",
    summary: "Scan every live HTTP endpoint of the cf.toml zones, Workers, tunnels and R2",
  },
  {
    name: "cf-r2-catalog",
    category: "data",
    kind: "tool",
    target: "tools/cf-r2-catalog.ts",
    summary: "Validate the R2 catalog SPDX data and optionally probe upstream APIs",
  },
  {
    name: "cf-reconcile",
    category: "cloud",
    kind: "tool",
    target: "tools/cf-reconcile.ts",
    summary: "Reconcile cf.toml with the live Cloudflare API (dry-run by default)",
  },
  {
    name: "cf-secrets",
    category: "infra",
    kind: "tool",
    target: "tools/cf-secrets.ts",
    summary: "Deduplicate and sync credentials and env files across the PC and the fleet",
  },
  {
    name: "cf-h3-probe",
    category: "cloud",
    kind: "tool",
    target: "tools/cf-h3-probe.ts",
    summary: "Probe every public host of cf.toml and edge.toml over HTTP/3 and record the negotiated protocol and Alt-Svc",
  },
  {
    name: "cf-skills-index",
    category: "data",
    kind: "tool",
    target: "tools/cf-skills-index.ts",
    summary: "Regenerate the agent-skills discovery index served by apps/cf-gateway",
  },
  {
    name: "cf-tunnels",
    category: "infra",
    kind: "tool",
    target: "tools/cf-tunnels.ts",
    summary: "Audit live cloudflared tunnels and their DNS records against cf.toml",
  },
  {
    name: "compose-check",
    category: "quality",
    kind: "tool",
    target: "tools/compose-check.ts",
    summary: "Build and test the Kotlin workspace (Aphrody Compose theme) on the build host",
  },
  {
    name: "map",
    category: "meta",
    kind: "tool",
    target: "tools/map.ts",
    summary: "Render MAP.tsv with the organization standard map tool",
  },
  {
    name: "tokens-compose",
    category: "data",
    kind: "tool",
    target: "tools/tokens-compose.ts",
    summary: "Render the Material 3 tokens as the Jetpack Compose theme, or check it is current",
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
