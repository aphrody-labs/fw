#!/usr/bin/env bun
import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join, resolve } from "node:path";

export interface MigrateOpenClawOptions {
  openclawDir?: string;
  configPath?: string;
  outputDir?: string;
  dryRun?: boolean;
  apply?: boolean;
}

export interface FwMigratedChannel {
  sourceName: string;
  fwChannelKind: "Cli" | "McpStdio" | "Acp" | "HttpWebhook" | "WebSocket" | "Cron";
  adapter: "native" | "http-webhook-bridge";
  enabled: boolean;
  parityGap?: string;
  rawKeys: string[];
}

export interface FwMigratedSkill {
  name: string;
  sourcePath: string;
  hasSkillMd: boolean;
  enabled: boolean;
}

export interface FwMigratedMcpServer {
  name: string;
  transport: "stdio" | "http" | "sse";
  command?: string;
  args?: string[];
  url?: string;
  bunRewritten: boolean;
  envKeys: string[];
  redactedSecretKeys: string[];
}

export interface BunRuntimePatchNote {
  upstreamPr: string;
  title: string;
  impact: string;
  fwMitigation: string;
}

export interface MigrateOpenClawResult {
  ok: boolean;
  mode: "dry-run" | "apply";
  openclawDir: string;
  configPath: string;
  configFound: boolean;
  outputDir: string;
  writtenFiles: string[];
  channels: FwMigratedChannel[];
  skills: FwMigratedSkill[];
  mcpServers: FwMigratedMcpServer[];
  gateway: {
    port: number;
    bind: string;
    defaultModel: string;
    disaggregatedPD: boolean;
    nodes: string[];
  };
  parityGaps: string[];
  bunRuntimePatchNotes: BunRuntimePatchNote[];
  warnings: string[];
}

export const OPENCLAW_BUN_RUNTIME_PATCH_NOTES: readonly BunRuntimePatchNote[] = [
  {
    upstreamPr: "openclaw/openclaw#201907",
    title: "fix: release retired agent catalogs on Bun",
    impact: "Long-lived agent catalog references can retain memory across hot reloads on Bun.",
    fwMitigation: "Fw RuntimeToolRegistry replaces Map entries in-place and increments versions without retaining old module graphs.",
  },
  {
    upstreamPr: "openclaw/openclaw#199959",
    title: "chore(runtime): update pinned Bun fork to 9debcc4b9",
    impact: "OpenClaw pins Node >=24.16 / pnpm@12.15 and tracks a custom Bun fork for runtime edge cases.",
    fwMitigation: "Fw targets stock Bun 1.4.5+ native APIs (Bun.Transpiler, bun:sqlite, Bun.CryptoHasher, Bun.gzipSync) with zero pnpm/Node dependencies.",
  },
  {
    upstreamPr: "openclaw/openclaw#186500",
    title: "fix(test): retain selected Bun through isolation",
    impact: "Isolated test runners can lose the selected Bun binary path across subprocesses.",
    fwMitigation: "Fw runs all tests under `bun test` with `bunfig.toml` root scoping and zero subprocess runtime drift.",
  },
  {
    upstreamPr: "openclaw/openclaw#165486",
    title: "feat: add signed bundled Bun to Windows desktop",
    impact: "Windows desktop bundles need clean file-lock teardown and native binary hygiene.",
    fwMitigation: "Fw PrefixKvRagStore finalizes every `bun:sqlite` Statement before `db.close()` so Windows temp/data dirs never leak EBUSY locks.",
  },
  {
    upstreamPr: "openclaw/openclaw#173373",
    title: "fix(cli): keep source commands on Node instead of switching to Bun",
    impact: "Mixed Node/Bun CLI dispatch introduces startup overhead and JSX/TSX loader mismatches.",
    fwMitigation: "Fw passes explicit `tsconfig` (`jsx: \"react\"`, `jsxFactory: \"__fwJsx\"`) to `Bun.Transpiler` and runs 100% on Bun.",
  },
] as const;

const SECRET_KEY_PATTERN = /(?:TOKEN|SECRET|API_KEY|PASSWORD|AUTH|CREDENTIAL|PRIVATE_KEY)/i;

function mapChannelToFw(name: string, cfg: Record<string, unknown>): FwMigratedChannel {
  const lower = name.toLowerCase();
  const enabled = cfg.enabled !== false;
  const rawKeys = Object.keys(cfg).sort();

  if (lower === "cli" || lower === "terminal") {
    return { sourceName: name, fwChannelKind: "Cli", adapter: "native", enabled, rawKeys };
  }
  if (lower === "mcp" || lower === "mcp-stdio") {
    return { sourceName: name, fwChannelKind: "McpStdio", adapter: "native", enabled, rawKeys };
  }
  if (lower === "acp" || lower === "acpx" || lower === "a2a") {
    return { sourceName: name, fwChannelKind: "Acp", adapter: "native", enabled, rawKeys };
  }
  if (lower === "ws" || lower === "websocket") {
    return { sourceName: name, fwChannelKind: "WebSocket", adapter: "native", enabled, rawKeys };
  }
  if (lower === "cron" || lower === "schedule") {
    return { sourceName: name, fwChannelKind: "Cron", adapter: "native", enabled, rawKeys };
  }
  if (lower === "webhook" || lower === "http") {
    return { sourceName: name, fwChannelKind: "HttpWebhook", adapter: "native", enabled, rawKeys };
  }

  return {
    sourceName: name,
    fwChannelKind: "HttpWebhook",
    adapter: "http-webhook-bridge",
    enabled,
    parityGap: `OpenClaw messaging channel "${name}" has no built-in native Rust transport in fw-hermes-claw; bridged via HttpWebhook.`,
    rawKeys,
  };
}

function rewriteCommandForBun(command: string, args: string[]): { command: string; args: string[]; bunRewritten: boolean } {
  const base = command.toLowerCase();
  if (base === "npx") {
    return { command: "bunx", args, bunRewritten: true };
  }
  if (base === "node") {
    return { command: "bun", args, bunRewritten: true };
  }
  if (base === "pnpm" && args[0] === "dlx") {
    return { command: "bunx", args: args.slice(1), bunRewritten: true };
  }
  return { command, args, bunRewritten: false };
}

function discoverSkills(openclawDir: string, configObj: Record<string, unknown>): FwMigratedSkill[] {
  const byName = new Map<string, FwMigratedSkill>();

  // 1. Skills declared in openclaw.json
  const skillsSection = (configObj.skills ?? {}) as Record<string, unknown>;
  const entries = (skillsSection.entries ?? skillsSection) as Record<string, unknown>;
  for (const [name, val] of Object.entries(entries)) {
    if (name === "load" || name === "allowBundled" || name === "entries") continue;
    const obj = typeof val === "object" && val !== null ? (val as Record<string, unknown>) : {};
    byName.set(name, {
      name,
      sourcePath: join(openclawDir, "skills", name),
      hasSkillMd: existsSync(join(openclawDir, "skills", name, "SKILL.md")),
      enabled: obj.enabled !== false,
    });
  }

  // 2. Skill directories on disk under ~/.openclaw/skills/*
  const skillsDir = join(openclawDir, "skills");
  if (existsSync(skillsDir)) {
    for (const entry of readdirSync(skillsDir, { withFileTypes: true })) {
      if (!entry.isDirectory()) continue;
      const skillDir = join(skillsDir, entry.name);
      const hasSkillMd = existsSync(join(skillDir, "SKILL.md"));
      const existing = byName.get(entry.name);
      if (existing) {
        existing.hasSkillMd = hasSkillMd;
        existing.sourcePath = skillDir;
      } else {
        byName.set(entry.name, {
          name: entry.name,
          sourcePath: skillDir,
          hasSkillMd,
          enabled: true,
        });
      }
    }
  }

  return Array.from(byName.values()).sort((a, b) => a.name.localeCompare(b.name));
}

function extractMcpServers(configObj: Record<string, unknown>): FwMigratedMcpServer[] {
  const rawServers =
    (configObj.mcpServers as Record<string, unknown> | undefined) ??
    ((configObj.mcp as Record<string, unknown> | undefined)?.servers as Record<string, unknown> | undefined) ??
    {};

  const out: FwMigratedMcpServer[] = [];
  for (const [name, value] of Object.entries(rawServers)) {
    if (typeof value !== "object" || value === null) continue;
    const srv = value as Record<string, unknown>;
    const envObj = (srv.env ?? {}) as Record<string, unknown>;
    const envKeys = Object.keys(envObj).sort();
    const redactedSecretKeys = envKeys.filter((k) => SECRET_KEY_PATTERN.test(k));

    if (typeof srv.url === "string") {
      out.push({
        name,
        transport: srv.transport === "sse" ? "sse" : "http",
        url: srv.url,
        bunRewritten: false,
        envKeys,
        redactedSecretKeys,
      });
      continue;
    }

    const rawCmd = typeof srv.command === "string" ? srv.command : "bunx";
    const rawArgs = Array.isArray(srv.args) ? srv.args.map(String) : [];
    const rewritten = rewriteCommandForBun(rawCmd, rawArgs);
    out.push({
      name,
      transport: "stdio",
      command: rewritten.command,
      args: rewritten.args,
      bunRewritten: rewritten.bunRewritten,
      envKeys,
      redactedSecretKeys,
    });
  }

  return out.sort((a, b) => a.name.localeCompare(b.name));
}

export function migrateOpenClawToFw(opts: MigrateOpenClawOptions = {}): MigrateOpenClawResult {
  const openclawDir = resolve(opts.openclawDir ?? join(homedir(), ".openclaw"));
  const configPath = resolve(opts.configPath ?? join(openclawDir, "openclaw.json"));
  const outputDir = resolve(opts.outputDir ?? join(homedir(), ".fw"));
  const mode: "dry-run" | "apply" = opts.apply === true && opts.dryRun !== true ? "apply" : "dry-run";
  const warnings: string[] = [];

  let configFound = false;
  let configObj: Record<string, unknown> = {};

  if (existsSync(configPath)) {
    configFound = true;
    try {
      configObj = JSON.parse(readFileSync(configPath, "utf8")) as Record<string, unknown>;
    } catch (err) {
      warnings.push(`Failed to parse ${configPath}: ${err instanceof Error ? err.message : String(err)}`);
    }
  } else {
    warnings.push(`OpenClaw config not found at ${configPath}; generating baseline Fw config.`);
  }

  // Channels
  const rawChannels = (configObj.channels ?? {}) as Record<string, unknown>;
  const channels: FwMigratedChannel[] = Object.entries(rawChannels)
    .filter(([, v]) => typeof v === "object" && v !== null)
    .map(([name, v]) => mapChannelToFw(name, v as Record<string, unknown>))
    .sort((a, b) => a.sourceName.localeCompare(b.sourceName));

  // Skills & MCP Servers
  const skills = discoverSkills(openclawDir, configObj);
  const mcpServers = extractMcpServers(configObj);

  // Gateway & model defaults
  const rawGateway = (configObj.gateway ?? {}) as Record<string, unknown>;
  const rawAgents = (configObj.agents ?? {}) as Record<string, unknown>;
  const rawDefaults = (rawAgents.defaults ?? {}) as Record<string, unknown>;
  const defaultModel =
    typeof rawDefaults.model === "string"
      ? rawDefaults.model
      : typeof (rawDefaults.model as Record<string, unknown> | undefined)?.primary === "string"
        ? String((rawDefaults.model as Record<string, unknown>).primary)
        : "fw-qwen3-8b-gguf";

  const gateway = {
    port: typeof rawGateway.port === "number" ? rawGateway.port : 18789,
    bind: typeof rawGateway.bind === "string" ? rawGateway.bind : "127.0.0.1",
    defaultModel,
    disaggregatedPD: true,
    nodes: ["local-rtx4070", "fr1", "fr2", "prd"],
  };

  const parityGaps: string[] = [];
  for (const ch of channels) {
    if (ch.parityGap) parityGaps.push(ch.parityGap);
  }
  if (channels.length === 0) {
    parityGaps.push(
      "OpenClaw ships native messaging channel extensions (WhatsApp, Telegram, Discord, Slack, Signal); Fw maps external chat platforms via HttpWebhook bridges.",
    );
  }

  const writtenFiles: string[] = [];
  if (mode === "apply") {
    mkdirSync(outputDir, { recursive: true });

    const fwConfigPath = join(outputDir, "fw.config.json");
    const fwMcpPath = join(outputDir, "fw.mcp.json");
    const fwChannelsPath = join(outputDir, "fw.channels.json");
    const fwSkillsPath = join(outputDir, "fw.skills.json");
    const fwReportPath = join(outputDir, "fw.migration-report.json");

    writeFileSync(
      fwConfigPath,
      JSON.stringify(
        {
          schema: "fw/v4.2",
          migratedFrom: configPath,
          runtime: "bun",
          gateway,
          plugin: "openclaw-fw",
        },
        null,
        2,
      ),
      "utf8",
    );
    writeFileSync(fwMcpPath, JSON.stringify({ mcpServers }, null, 2), "utf8");
    writeFileSync(fwChannelsPath, JSON.stringify({ channels }, null, 2), "utf8");
    writeFileSync(fwSkillsPath, JSON.stringify({ skills }, null, 2), "utf8");

    writtenFiles.push(fwConfigPath, fwMcpPath, fwChannelsPath, fwSkillsPath, fwReportPath);

    writeFileSync(
      fwReportPath,
      JSON.stringify(
        {
          ok: true,
          mode,
          migratedAt: new Date().toISOString(),
          parityGaps,
          bunRuntimePatchNotes: OPENCLAW_BUN_RUNTIME_PATCH_NOTES,
          warnings,
        },
        null,
        2,
      ),
      "utf8",
    );
  }

  return {
    ok: true,
    mode,
    openclawDir,
    configPath,
    configFound,
    outputDir,
    writtenFiles,
    channels,
    skills,
    mcpServers,
    gateway,
    parityGaps,
    bunRuntimePatchNotes: [...OPENCLAW_BUN_RUNTIME_PATCH_NOTES],
    warnings,
  };
}

if (import.meta.main) {
  const argv = process.argv.slice(2);
  const opts: MigrateOpenClawOptions = { dryRun: true };
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === "--apply") {
      opts.apply = true;
      opts.dryRun = false;
    } else if (arg === "--dry-run") {
      opts.dryRun = true;
      opts.apply = false;
    } else if (arg === "--openclaw-dir" && argv[i + 1]) {
      opts.openclawDir = argv[++i];
    } else if (arg === "--config" && argv[i + 1]) {
      opts.configPath = argv[++i];
    } else if (arg === "--output-dir" && argv[i + 1]) {
      opts.outputDir = argv[++i];
    }
  }
  const result = migrateOpenClawToFw(opts);
  console.log(JSON.stringify(result, null, 2));
}
