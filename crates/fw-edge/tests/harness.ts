// Starts `wrangler dev` for the Rust fw-edge build and gives tests its base URL.
// Usage (from crates/fw-edge, after `worker-build --release`):
//   const edge = await startEdge({ vars: { ACCESS_TEAM_DOMAIN: mock.url } });  ...  await edge.stop();
// The AI binding is remote-only in `wrangler dev`; scenarios that need Workers AI run against the preview URL.
import { spawn, spawnSync, type ChildProcess } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

export interface Edge {
  url: string;
  stop(): Promise<void>;
}

export async function startEdge(opts: { port?: number; vars?: Record<string, string>; persistTo?: string } = {}): Promise<Edge> {
  const port = opts.port ?? 8800 + Math.floor(Math.random() * 500);
  const cwd = fileURLToPath(new URL("..", import.meta.url));
  // `wrangler dev` would rerun `build.command` (worker-build, a heavy cargo build): tests run the build output
  // that already exists, so they use a copy of the config without the build step.
  const config = JSON.parse(readFileSync(new URL("../wrangler.jsonc", import.meta.url), "utf8").replace(/^\s*\/\/.*$/gm, ""));
  delete config.build;
  writeFileSync(new URL("../wrangler.test.json", import.meta.url), JSON.stringify(config));
  const args = ["dev", "--config", "wrangler.test.json", "--local", "--port", String(port), "--ip", "127.0.0.1"];
  if (opts.persistTo) args.push("--persist-to", opts.persistTo);
  for (const [k, v] of Object.entries(opts.vars ?? {})) args.push("--var", `${k}:${v}`);
  const child: ChildProcess = spawn(process.env.WRANGLER ?? "wrangler", args, { cwd, stdio: ["ignore", "pipe", "pipe"], env: { ...process.env, CI: "1" } });
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
      if (process.platform === "win32" && child.pid) spawnSync("taskkill", ["/PID", String(child.pid), "/T", "/F"]);
      else child.kill();
      if (child.exitCode === null) await new Promise((r) => child.once("exit", r));
    },
  };
}
