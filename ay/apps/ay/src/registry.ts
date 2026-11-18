import { join, resolve } from "node:path";

export type CommandKind = "app" | "package" | "tool" | "builtin";

export interface CommandEntry {
  readonly name: string;
  readonly aliases: readonly string[];
  readonly kind: CommandKind;
  readonly category: "web" | "ai" | "shell" | "re" | "wasm" | "cloud" | "sdk";
  readonly summary: string;
  readonly entry?: string;
}

export const COMMAND_REGISTRY: readonly CommandEntry[] = [
  {
    name: "web",
    aliases: ["webos", "portal", "site"],
    kind: "app",
    category: "web",
    summary: "Serve unified Aphrody Website, WebOS shell, Search gateway, WASM corelib sandbox, and Chromium Dash.",
    entry: "apps/web/src/main.ts",
  },
  {
    name: "dash",
    aliases: ["chromium-dash"],
    kind: "app",
    category: "web",
    summary: "Run standalone Chromium Dash & Chrome Status SSR server.",
    entry: "apps/chromium-dash/src/main.ts",
  },
  {
    name: "chromium",
    aliases: ["chromium-releases", "chrome"],
    kind: "package",
    category: "web",
    summary: "Query Chromium release channels, schedules, commits, features, and CEF pins.",
    entry: "packages/chromium-releases/src/cli.ts",
  },
  {
    name: "ai",
    aliases: ["os", "ai-os", "agent"],
    kind: "package",
    category: "ai",
    summary: "AI OS kernel: agent runtime, MCP/A2A server, orchestrator, fabric, brain, providers, NHITL, automate.",
    entry: "packages/ai-os/src/cli.ts",
  },
  {
    name: "shell",
    aliases: ["sh", "ay-sh", "shells"],
    kind: "tool",
    category: "shell",
    summary: "Inspect/launch native shells (ay-sh, fish, nu, bash, niubash, pwsh) and platform matrix.",
    entry: "tools/shells.ts",
  },
  {
    name: "omniu",
    aliases: ["prompt", "niu"],
    kind: "package",
    category: "shell",
    summary: "Oh-My-Niu / shell themes, completions, prompts, and command packs.",
    entry: "packages/omniu/src/cli.ts",
  },
  {
    name: "gnu",
    aliases: ["coreutils", "unix"],
    kind: "package",
    category: "shell",
    summary: "Cross-platform GNU / uutils userland runner and parity suite.",
    entry: "packages/gnu/src/cli.ts",
  },
  {
    name: "re",
    aliases: ["ghidra", "disasm"],
    kind: "package",
    category: "re",
    summary: "Reverse engineering suite: binary triage, Ghidra headless/BUN, PE/ELF/Lua52/CPK analysis.",
    entry: "packages/ghidra/src/cli.ts",
  },
  {
    name: "py2bun",
    aliases: [],
    kind: "package",
    category: "sdk",
    summary: "Python-to-Bun AST transpiler and stdlib compatibility layer.",
    entry: "packages/py2bun/src/cli.ts",
  },
  {
    name: "wck",
    aliases: ["web-check"],
    kind: "package",
    category: "web",
    summary: "Modern Web Guidance, WCAG 2.2 AA, SEO, and Core Web Vitals conformance checker.",
    entry: "packages/wck/src/cli.ts",
  },
  {
    name: "wasm",
    aliases: ["corelib"],
    kind: "tool",
    category: "wasm",
    summary: "Run corelib WebAssembly component conformance host and boot verification.",
    entry: "tools/corelib-component.ts",
  },
  {
    name: "qemu",
    aliases: ["kernel-boot"],
    kind: "tool",
    category: "wasm",
    summary: "Boot freestanding x86_64 corelib PVH image in QEMU and verify serial transcript.",
    entry: "tools/corelib-qemu.ts",
  },
  {
    name: "abi",
    aliases: ["codegen"],
    kind: "tool",
    category: "wasm",
    summary: "Generate and check unified C23 / Rust / Bun FFI / Python / JSON ABI artifacts from abi/ay.abi.toml.",
    entry: "tools/abi.ts",
  },
  {
    name: "deploy",
    aliases: ["cloud"],
    kind: "tool",
    category: "cloud",
    summary: "Stage-aware Cloudflare Workers/Gatekeepers & fleet service deploy planner.",
    entry: "tools/deploy.ts",
  },
  {
    name: "quota",
    aliases: ["cf-quota", "limits"],
    kind: "tool",
    category: "cloud",
    summary: "Quota-safe routing monitor across Cloudflare Workers/R2/KV/D1, GitHub Actions, and VPS fleet.",
    entry: "tools/cf-quota.ts",
  },
  {
    name: "mirror",
    aliases: ["org-mirror"],
    kind: "package",
    category: "sdk",
    summary: "Mirror and audit upstream organizational repositories and forks.",
    entry: "packages/org-mirror/src/cli.ts",
  },
  {
    name: "features",
    aliases: ["matrix"],
    kind: "tool",
    category: "sdk",
    summary: "Render and validate docs/features.toml product proof matrix.",
    entry: "tools/features.ts",
  },
  {
    name: "submodules",
    aliases: ["forks"],
    kind: "tool",
    category: "sdk",
    summary: "Reconcile .gitmodules and gitlinks against submodules.toml.",
    entry: "tools/submodules.ts",
  },
  {
    name: "inventory",
    aliases: ["orgs"],
    kind: "tool",
    category: "sdk",
    summary: "Inventory repositories, packages, and org transfers against ay.toml.",
    entry: "tools/org-inventory.ts",
  },
  {
    name: "hosts",
    aliases: [],
    kind: "tool",
    category: "cloud",
    summary: "List configured fleet host roles and reachability from ay.toml / ay.local.toml.",
    entry: "tools/hosts.ts",
  },
  {
    name: "disks",
    aliases: [],
    kind: "tool",
    category: "cloud",
    summary: "Measure host build/test/prod volume headroom.",
    entry: "tools/disks.ts",
  },
  {
    name: "status",
    aliases: ["info"],
    kind: "builtin",
    category: "sdk",
    summary: "Print unified product manifest, workspace packages/crates counts, and fast tool telemetry.",
  },
  {
    name: "doctor",
    aliases: ["health"],
    kind: "builtin",
    category: "sdk",
    summary: "Check fast modern CLI tools (bun, eza, dust, xh, fd, rg, fish, nu, bat, wrangler, clang, cargo, gh) and workspace health.",
  },
  {
    name: "check",
    aliases: ["verify"],
    kind: "builtin",
    category: "sdk",
    summary: "Run full workspace verification pipeline (`bun run check`).",
  },
] as const;

export const FAST_TOOL_BINARIES = [
  "bun",
  "cargo",
  "clang",
  "gh",
  "wrangler",
  "eza",
  "dust",
  "xh",
  "fd",
  "rg",
  "fish",
  "nu",
  "bat",
] as const;

export function resolveRepoRoot(fromDir = import.meta.dir): string {
  return resolve(fromDir, "../../..");
}

export function findCommand(nameOrAlias: string): CommandEntry | undefined {
  const key = nameOrAlias.trim().toLowerCase();
  return COMMAND_REGISTRY.find((c) => c.name === key || c.aliases.includes(key));
}

export function probeFastTools(): Record<(typeof FAST_TOOL_BINARIES)[number], string | null> {
  const out = {} as Record<(typeof FAST_TOOL_BINARIES)[number], string | null>;
  for (const bin of FAST_TOOL_BINARIES) {
    out[bin] = Bun.which(bin) ?? null;
  }
  return out;
}

export async function readUnifiedStatus(root = resolveRepoRoot()) {
  const ayText = await Bun.file(join(root, "ay.toml")).text();
  const ay = Bun.TOML.parse(ayText) as Record<string, any>;
  const rootPkg = (await Bun.file(join(root, "package.json")).json()) as {
    name?: string;
    bin?: Record<string, string>;
    workspaces?: { packages?: string[] };
  };
  return {
    product: ay.product ?? { name: "aphrody", cli: "ay" },
    orgs: ay.orgs ?? {},
    domains: ay.domains ?? {},
    products: Object.keys(ay.products ?? {}),
    repos: Object.keys(ay.repos ?? {}),
    sdk: ay.sdk ?? {},
    cliBin: rootPkg.bin ?? {},
    commands: COMMAND_REGISTRY.map((c) => c.name),
    tools: probeFastTools(),
  };
}
