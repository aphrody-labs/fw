// Unified Bun-native runtime adapter for our patched Cloudflare & Vercel open-source forks
// (`third_party/cloudflare/*` and `third_party/vercel/*`, pinned in `submodules.toml`).
// Zero npm bloat: pure Web-standard Streams, Bun crypto/hashing, and Durable-Object / Workflow envelopes.

export interface GatekeeperBinding {
  readonly id: string;
  readonly workerName: string;
  readonly upstreamOrigin: string;
  readonly allowedScopes: readonly string[];
}

export const CLOUDFLARE_OS_GATEKEEPERS: readonly GatekeeperBinding[] = [
  { id: "github", workerName: "ay-gatekeeper-github-prod", upstreamOrigin: "https://api.github.com", allowedScopes: ["repo:read", "checks:read"] },
  { id: "cloudflare", workerName: "ay-gatekeeper-cloudflare-prod", upstreamOrigin: "https://api.cloudflare.com", allowedScopes: ["workers:read", "r2:rw"] },
  { id: "gemini", workerName: "ay-gatekeeper-gemini-prod", upstreamOrigin: "https://generativelanguage.googleapis.com", allowedScopes: ["models:generate"] },
  { id: "anthropic", workerName: "ay-gatekeeper-anthropic-prod", upstreamOrigin: "https://api.anthropic.com", allowedScopes: ["messages:create"] },
  { id: "openai", workerName: "ay-gatekeeper-openai-prod", upstreamOrigin: "https://api.openai.com", allowedScopes: ["responses:create"] },
] as const;

export interface GadgetManifest {
  readonly id: string;
  readonly workshopClass: string;
  readonly gatekeepers: readonly string[];
  readonly webgpu: boolean;
}

/** Encodes a Vercel AI SDK v5/v6 UIMessage SSE data-stream frame (`data: {...}\n\n`). */
export function encodeAiStreamPart(part: { type: "text-delta" | "tool-call" | "finish"; id?: string; delta?: string; toolName?: string; args?: unknown }): string {
  return `data: ${JSON.stringify(part)}\n\n`;
}

/** Parses SSE frames produced by `encodeAiStreamPart` or `@ai-sdk/ui-utils`. */
export function decodeAiStreamChunk(chunk: string): Array<Record<string, unknown>> {
  const out: Array<Record<string, unknown>> = [];
  for (const line of chunk.split("\n")) {
    const trimmed = line.trim();
    if (!trimmed.startsWith("data:")) continue;
    const payload = trimmed.slice(5).trim();
    if (!payload || payload === "[DONE]") continue;
    out.push(JSON.parse(payload) as Record<string, unknown>);
  }
  return out;
}

/** Deterministic step memoization ledger matching `aphrody-labs/workflow` (`vercel/workflow` Bun fork). */
export class DurableWorkflowRun {
  readonly runId: string;
  private readonly steps = new Map<string, unknown>();

  constructor(runId: string) {
    this.runId = runId;
  }

  async step<T>(name: string, fn: () => T | Promise<T>): Promise<T> {
    if (this.steps.has(name)) {
      return this.steps.get(name) as T;
    }
    const result = await fn();
    this.steps.set(name, result);
    return result;
  }

  completedSteps(): string[] {
    return [...this.steps.keys()];
  }
}

/** Sandboxed virtual command evaluator bridging `vercel-labs/just-bash` + `wterm` + `nysh` without spawning OS shells. */
export function evalVirtualNyshPipeline(pipeline: string, vfs: ReadonlyMap<string, string>): { stdout: string; exitCode: number } {
  const stages = pipeline.split("|").map((s) => s.trim()).filter(Boolean);
  let buf = "";
  for (const stage of stages) {
    const [cmd, ...rest] = stage.split(/\s+/);
    if (cmd === "echo") {
      buf = rest.join(" ").replace(/^["']|["']$/g, "") + "\n";
    } else if (cmd === "cat") {
      const path = rest[0] ?? "";
      const content = vfs.get(path);
      if (content === undefined) return { stdout: `cat: ${path}: No such file\n`, exitCode: 1 };
      buf = content;
    } else if (cmd === "grep") {
      const needle = (rest[0] ?? "").replace(/^["']|["']$/g, "");
      buf = buf
        .split("\n")
        .filter((l) => l.includes(needle))
        .join("\n");
      if (buf) buf += "\n";
    } else if (cmd === "wc" && rest[0] === "-l") {
      const count = buf ? buf.split("\n").filter((l, i, arr) => i < arr.length - 1 || l.length > 0).length : 0;
      buf = `${count}\n`;
    } else {
      return { stdout: `nysh-virtual: unsupported command: ${cmd}\n`, exitCode: 127 };
    }
  }
  return { stdout: buf, exitCode: 0 };
}
