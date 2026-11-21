// Starts `wrangler dev` for the Rust fw-edge build and gives tests its base URL.
// Usage (from crates/fw-edge, after `worker-build --release`):
//   const edge = await startEdge({ vars: { ACCESS_TEAM_DOMAIN: mock.url } });  ...  await edge.stop();
// The AI binding is remote-only in `wrangler dev`; scenarios that need Workers AI run against the preview URL.
import { spawn, type ChildProcess } from "node:child_process";
import { fileURLToPath } from "node:url";

export interface Edge {
  url: string;
  stop(): Promise<void>;
}

export async function startEdge(opts: { port?: number; vars?: Record<string, string>; persistTo?: string } = {}): Promise<Edge> {
  const port = opts.port ?? 8800 + Math.floor(Math.random() * 500);
  const cwd = fileURLToPath(new URL("..", import.meta.url));
  const args = ["dev", "--config", "wrangler.jsonc", "--local", "--port", String(port), "--ip", "127.0.0.1"];
  if (opts.persistTo) args.push("--persist-to", opts.persistTo);
  for (const [k, v] of Object.entries(opts.vars ?? {})) args.push("--var", `${k}:${v}`);
  const child: ChildProcess = spawn("wrangler", args, { cwd, stdio: ["ignore", "pipe", "pipe"], env: { ...process.env, CI: "1" } });
  let log = "";
  child.stdout?.on("data", (d) => (log += d));
  child.stderr?.on("data", (d) => (log += d));
  const url = `http://127.0.0.1:${port}`;
  const deadline = Date.now() + 120_000;
  for (;;) {
    if (child.exitCode !== null) throw new Error(`wrangler dev exited ${child.exitCode}\n${log}`);
    try {
      if ((await fetch(`${url}/health`)).status > 0) break;
    } catch {
      // not listening yet
    }
    if (Date.now() > deadline) {
      child.kill();
      throw new Error(`wrangler dev did not start\n${log}`);
    }
    await new Promise((r) => setTimeout(r, 500));
  }
  return {
    url,
    async stop() {
      child.kill();
      await new Promise((r) => child.once("exit", r));
    },
  };
}
