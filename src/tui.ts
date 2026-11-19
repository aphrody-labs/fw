import { probeRustAbi, readTargetManifest } from "./targets.ts";
import { pluginStatus } from "./plugin_sync.ts";
import { serviceStatus } from "./daemon.ts";
import { listVfsMounts } from "./vfs.ts";
import { DEFAULT_ROUTER_PROVIDERS, loadSoul } from "./ai_stack.ts";

export function renderFwTui(root: string): string {
  const abi = probeRustAbi(root);
  const manifest = readTargetManifest(root);
  const plugin = pluginStatus(root);
  const svc = serviceStatus(root);
  const vfs = listVfsMounts(root);
  const soul = loadSoul(root);
  const identity = (soul.identity ?? {}) as { name?: string; version?: string };

  const lines = [
    "======================================================================",
    `  FW OS TUI  |  ${identity.name ?? "Fw"} v${identity.version ?? "2.4.0"}  |  ${process.platform}/${process.arch}`,
    "======================================================================",
    `  Root       : ${root}`,
    `  Rust ABI   : ${abi.loaded ? `v${abi.version} (${abi.health?.lto ?? "unknown"} LTO)` : "not loaded"}`,
    `  Grammars   : ${Array.isArray(abi.health?.treeSitterGrammars) ? (abi.health.treeSitterGrammars as string[]).join(", ") : "n/a"}`,
    `  Plugin     : fw (${plugin.skills.length} skills, overlap=${plugin.dedupe.overlapWithAphrody.length}, junctions=${plugin.dedupe.ntfsJunctionsFound.length})`,
    `  Daemon     : ${svc.installed ? "INSTALLED (boot-startup + auto-restart)" : "not installed"}`,
    `  Manifest   : ${manifest ? `repoDigest=${manifest.repoDigest.slice(0, 16)}... (${manifest.totalTrackedFiles} files)` : "pending scan"}`,
    `  VFS Mounts : ${vfs.mounts.map((m) => m.mount).join(", ")}`,
    `  AI Router  : ${DEFAULT_ROUTER_PROVIDERS.map((p) => p.id).join(" -> ")}`,
    "----------------------------------------------------------------------",
    "  Surfaces   : CLI | TUI | Web/GUI | API (OpenAI/OpenAPI/MCP/ACP/A2A) | SDK | Provider | Cloudflare",
    "======================================================================",
  ];
  return lines.join("\n");
}
