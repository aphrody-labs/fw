#!/usr/bin/env bun
// End-to-end check of fw: every CLI command (`fw help <cmd>` plus one safe invocation), `fw serve` routes, the fw MCP
// server, the aphrody plugin hooks and `fw upgrade`. Everything that writes runs against a temporary HOME and FX_HOME.
// Usage: bun fw-e2e.ts [--fw <bin>] [--root <fw repo>] [--only cli,serve,mcp,hooks,upgrade] [--fast] [--json]
import { spawn, spawnSync } from "node:child_process";
import { copyFileSync, cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { delimiter, dirname, join, resolve } from "node:path";
import { createHash } from "node:crypto";

type Section = "cli" | "serve" | "mcp" | "hooks" | "upgrade";
type Verdict = "ok" | "fail";
type Row = { section: Section; name: string; cmd: string; result: Verdict; ms: number; note: string };

const IS_WIN = process.platform === "win32";
const EXE = IS_WIN ? ".exe" : "";

const argv = Bun.argv.slice(2);
const flag = (name: string): string | undefined => {
  const i = argv.indexOf(name);
  return i >= 0 ? argv[i + 1] : undefined;
};
const JSON_OUT = argv.includes("--json");
const FAST = argv.includes("--fast");
const only = (flag("--only") ?? "").split(",").filter(Boolean) as Section[];
const wanted = (s: Section) => only.length === 0 || only.includes(s);

function resolveRoot(): string {
  const marker = (d: string) => existsSync(join(d, "fw.targets.json")) && existsSync(join(d, "crates", "fw-bin"));
  const candidates: string[] = [];
  for (const v of [flag("--root"), process.env.FW_ROOT]) if (v) candidates.push(resolve(v));
  for (let d = import.meta.dir, i = 0; i < 6; i++, d = dirname(d)) candidates.push(d);
  if (process.env.WORKSPACE_ROOT) candidates.push(join(process.env.WORKSPACE_ROOT, "fw"));
  candidates.push(join(process.env.USERPROFILE ?? process.env.HOME ?? homedir(), "src", "fw"));
  const found = candidates.find(marker);
  if (!found) throw new Error(`fw repository not found (tried ${candidates.join(", ")}); pass --root`);
  return found;
}

const ROOT = resolveRoot();

function resolveFwBin(): string {
  const explicit = flag("--fw") ?? process.env.FW_BIN;
  if (explicit) return resolve(explicit);
  const candidates = [join(ROOT, "target", "release", `fw${EXE}`), join(ROOT, "target", "debug", `fw${EXE}`)];
  const found = candidates.find(existsSync) ?? Bun.which("fw");
  if (!found) throw new Error("fw binary not found; pass --fw <path> or build it with cargo build -p fw-bin");
  return resolve(found);
}

const FW = resolveFwBin();
const FW_DIR = dirname(FW);

// ---- sandbox ---------------------------------------------------------------------------------------------------

const SANDBOX = mkdtempSync(join(tmpdir(), "fw-e2e-"));
const HOME_DIR = join(SANDBOX, "home");
const WORK = join(SANDBOX, "work");
const OUT = join(SANDBOX, "out");
for (const d of [HOME_DIR, WORK, OUT, join(HOME_DIR, ".local", "bin")]) mkdirSync(d, { recursive: true });

const SECRET_ENV = /(TOKEN|API_KEY|SECRET|PASSWORD|CREDENTIAL)/i;
function sandboxEnv(extra: Record<string, string> = {}): Record<string, string> {
  const env: Record<string, string> = {};
  for (const [k, v] of Object.entries(process.env)) if (v !== undefined && !SECRET_ENV.test(k)) env[k] = v;
  return {
    ...env,
    HOME: HOME_DIR,
    USERPROFILE: HOME_DIR,
    APPDATA: join(HOME_DIR, "AppData", "Roaming"),
    LOCALAPPDATA: join(HOME_DIR, "AppData", "Local"),
    XDG_CONFIG_HOME: join(HOME_DIR, ".config"),
    XDG_DATA_HOME: join(HOME_DIR, ".local", "share"),
    XDG_CACHE_HOME: join(HOME_DIR, ".cache"),
    FX_HOME: join(HOME_DIR, ".fx"),
    APHRODY_HOME: join(HOME_DIR, ".aphrody"),
    FW_LEARN_XFS_ROOT: join(HOME_DIR, "xfs"),
    NO_COLOR: "1",
    ...extra,
  };
}

let seeded = false;
function seedWorkspace(): void {
  if (seeded) return;
  seeded = true;
  const files: Record<string, string> = {
    "Cargo.toml": '[package]\nname = "demo"\nversion = "0.1.0"\nedition = "2021"\n',
    "src/lib.rs": "pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n",
    "package.json": '{"name":"demo","version":"1.0.0","dependencies":{}}\n',
    "a.txt": "hello fw\nsecond line\n",
    "data.json": '{"a":{"b":[1,2,3]}}\n',
    "README.md": "# demo\n\nA demo workspace for the fw end-to-end run.\n",
    ".claude/skills/demo/SKILL.md": "---\nname: demo\ndescription: Demo skill used by the fw end-to-end run.\n---\n\n# demo\n",
  };
  for (const [rel, body] of Object.entries(files)) {
    const p = join(WORK, rel);
    mkdirSync(dirname(p), { recursive: true });
    writeFileSync(p, body);
  }
  const git = (...a: string[]) => spawnSync("git", ["-c", "user.name=e2e", "-c", "user.email=e2e@example.invalid", ...a], { cwd: WORK, stdio: "ignore" });
  git("init", "-q");
  git("add", "-A");
  git("commit", "-q", "-m", "seed");
}

// ---- process runner --------------------------------------------------------------------------------------------

type Run = { code: number | null; out: string; err: string; ms: number; timedOut: boolean };

function runProcess(cmd: string[], opts: { cwd?: string; env?: Record<string, string>; stdin?: string; timeoutMs?: number } = {}): Promise<Run> {
  return new Promise((done) => {
    const started = performance.now();
    const child = spawn(cmd[0], cmd.slice(1), {
      cwd: opts.cwd ?? WORK,
      env: opts.env ?? sandboxEnv(),
      stdio: ["pipe", "pipe", "pipe"],
      windowsHide: true,
    });
    let out = "";
    let err = "";
    let timedOut = false;
    const timer = setTimeout(() => {
      timedOut = true;
      child.kill("SIGKILL");
    }, opts.timeoutMs ?? 30_000);
    child.stdout.on("data", (d) => (out += d));
    child.stderr.on("data", (d) => (err += d));
    child.on("error", (e) => {
      err += String(e);
    });
    child.stdin.on("error", () => {});
    child.stdin.end(opts.stdin ?? "");
    child.on("close", (code) => {
      clearTimeout(timer);
      done({ code, out, err, ms: Math.round(performance.now() - started), timedOut });
    });
  });
}

const PANIC = /panicked at|RUST_BACKTRACE|stack overflow|thread '.*' panicked|STATUS_STACK|Segmentation fault/i;
const short = (s: string, n = 110) => s.replace(/\s+/g, " ").trim().slice(0, n);

const rows: Row[] = [];
function record(section: Section, name: string, cmd: string, result: Verdict, ms: number, note = ""): void {
  rows.push({ section, name, cmd, result, ms, note });
  if (!JSON_OUT) console.error(`${result === "ok" ? "ok  " : "FAIL"} ${section}/${name} ${ms}ms ${result === "fail" ? note : ""}`);
}

// ---- CLI -------------------------------------------------------------------------------------------------------

type CliCase = {
  /** Command argv after `fw`. */
  args: string[];
  /** 0 (default): exit 0. "fail": a clean nonzero exit with a message. */
  exit?: 0 | "fail";
  stdin?: string;
  out?: RegExp;
  err?: RegExp;
  cwd?: string;
  env?: Record<string, string>;
  timeoutMs?: number;
  /** Extra check on the run result; returns a failure note. */
  check?: (r: Run) => string | undefined;
};

function judgeCli(c: CliCase, r: Run): string | undefined {
  if (r.timedOut) return "timeout";
  if (r.code === null) return "killed by signal";
  if (r.code === 101 || PANIC.test(r.err) || PANIC.test(r.out)) return `panic: ${short(r.err || r.out)}`;
  if (c.exit === "fail") {
    if (r.code === 0) return `expected a nonzero exit, got 0: ${short(r.out)}`;
    if (!r.err.trim() && !r.out.trim()) return "nonzero exit without a message";
  } else if (r.code !== 0) {
    return `exit ${r.code}: ${short(r.err || r.out)}`;
  }
  if (c.out && !c.out.test(r.out)) return `stdout lacks ${c.out}: ${short(r.out)}`;
  if (c.err && !c.err.test(r.err + r.out)) return `stderr lacks ${c.err}: ${short(r.err)}`;
  return c.check?.(r);
}

async function runCli(name: string, c: CliCase, bin = FW, section: Section = "cli"): Promise<void> {
  const r = await runProcess([bin, ...c.args], { cwd: c.cwd, env: sandboxEnv(c.env), stdin: c.stdin, timeoutMs: c.timeoutMs });
  const note = judgeCli(c, r);
  const shown = `${bin === FW ? "fw" : "<temp>/fw"} ${c.args.map((a) => (/[\s'"{]/.test(a) ? JSON.stringify(a) : a)).join(" ")}`.replaceAll(SANDBOX, "$SANDBOX");
  record(section, name, shown, note ? "fail" : "ok", r.ms, note ?? (c.exit === "fail" ? `exit ${r.code}: ${short(r.err || r.out, 70)}` : ""));
}

/** Command names and aliases from `fw --help`, so a command added to cli_parser.rs without a case here fails the run. */
async function helpCommands(): Promise<{ name: string; aliases: string[] }[]> {
  const r = await runProcess([FW, "--help"]);
  const list: { name: string; aliases: string[] }[] = [];
  for (const line of r.out.split("\n")) {
    const m = /^ {2}([a-z][a-z0-9-]*)(?: \(([^)]*)\))?\s{2,}\S/.exec(line);
    if (m) list.push({ name: m[1], aliases: (m[2] ?? "").split(",").map((s) => s.trim()).filter(Boolean) });
  }
  return list;
}

/** A command run by a sibling binary: without it next to fw the command must say how to build it and exit nonzero. */
function siblingCase(binary: string, available: CliCase): CliCase {
  return existsSync(join(FW_DIR, `${binary}${EXE}`)) ? available : { args: available.args, exit: "fail", err: new RegExp(binary) };
}

const LEGACY = ["status", "hardware", "compute", "ggml", "openapi", "ui", "ostd", "uutils", "nix", "stack", "wayland", "claw", "migrate"];

function cliCases(): Record<string, CliCase> {
  const binDir = join(OUT, "bin");
  const projectDir = join(OUT, "project");
  const offline = ["--offline"];
  return {
    ask: { args: [...offline, "ask", "say hi"], exit: "fail", err: /./ },
    tool: { args: ["tool", "read_file", '{"path":"a.txt"}'], out: /hello fw/ },
    tui: { args: ["tui"], timeoutMs: 15_000 },
    // fw-video demuxes video: a Markdown file must be refused with a message, not crash.
    convert: { args: ["convert", "README.md", join(OUT, "convert"), "--to", "html"], exit: "fail", err: /./ },
    google: { args: [...offline, "google", "drive", "list"], exit: "fail", err: /./ },
    files: { args: ["files", "ls", WORK], out: /a\.txt/ },
    serve: { args: ["serve", "not-an-address"], exit: "fail", err: /host:port/ },
    infer: { args: [...offline, "infer", "hello", "--url", "http://127.0.0.1:1/v1"], exit: "fail", err: /./ },
    train: { args: ["train", "--steps", "5", "--out", join(OUT, "lora.json")], out: /./ },
    hf: { args: [...offline, "hf", "whoami"], exit: "fail", err: /./ },
    abliterate: { args: ["abliterate", "plan"], out: /./ },
    vram: { args: ["vram", "--vram-gb", "12", "--model-gb", "5.5", "--layers", "32", "--ctx", "8192"], out: /./ },
    repomap: { args: ["repomap", "."], out: /add/ },
    watch: { args: ["watch", "list"], out: /./ },
    tune: { args: ["tune", "hardware"], out: /./ },
    sec: { args: ["sec", "."], out: /./ },
    cluster: { args: ["cluster", "route", "hello"], out: /first/ },
    "cluster k8s": { args: ["cluster", "k8s", "status"], exit: "fail", err: /./ },
    graph: siblingCase("fw-graph", { args: ["graph", "build", ".", "--json", join(OUT, "graph.json")], out: /./ }),
    mem: { args: ["mem", "stats"], out: /./ },
    docs: { args: [...offline, "docs", "list"], out: /./ },
    learn: { args: ["learn", "status", "--xfs-root", join(OUT, "xfs")], out: /./ },
    skills: { args: [...offline, "skills", "list"], out: /demo/ },
    onboard: { args: ["onboard", "."], out: /./ },
    workflow: { args: ["workflow", "list"], out: /discover/ },
    "self-install": { args: ["self-install", "--dir", binDir], out: /"installed"/ },
    "self-update": { args: [...offline, "self-update", "--check"], exit: "fail", err: /offline/ },
    "self-uninstall": { args: ["self-uninstall", "--dir", binDir], out: /"uninstalled": true/ },
    sh: { args: ["sh", "-c", "echo hello | wc -c"], out: /\d/ },
    jq: { args: ["jq", ".a.b[1]", "data.json"], out: /2/ },
    tokei: { args: ["tokei", "."], out: /./ },
    du: { args: ["du", "."], out: /./ },
    bat: { args: ["bat", "a.txt"], out: /hello fw/ },
    heretic: { args: ["heretic", "presets"], out: /./ },
    align: { args: ["align", "dpo", "--steps", "5"], out: /./ },
    init: { args: ["init"], cwd: projectDir, out: /./ },
    install: { args: [...offline, "install"], cwd: projectDir, out: /./ },
    add: { args: [...offline, "add", "left-pad"], cwd: projectDir, out: /./ },
    remove: { args: ["remove", "left-pad"], cwd: projectDir, out: /./ },
    lock: { args: ["lock", "."], out: /./ },
    verify: { args: ["verify", "."], out: /./ },
    info: { args: [...offline, "info", "cargo", "serde"], exit: "fail", err: /offline|network|resolve|fetch/i },
    fetch: { args: [...offline, "fetch", "cargo", "serde"], exit: "fail", err: /offline|network|resolve|fetch|download/i },
    tree: { args: ["tree", "."], out: /./ },
    export: { args: ["export", "."], out: /CycloneDX/ },
    audit: { args: [...offline, "audit"], cwd: projectDir, out: /./ },
    compile: { args: ["compile"], cwd: projectDir, out: /./ },
    semver: { args: ["semver", "1.2.3", "^1.0.0"], out: /true/i },
    config: { args: ["config", "set", "model", "e2e-model"], out: /./ },
    mcp: { args: ["mcp", "list"], out: /./ },
    acp: { args: ["acp"], stdin: '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":1}}\n', out: /jsonrpc/ },
    sessions: { args: ["sessions"], out: /./ },
    permissions: { args: ["permissions"], out: /./ },
    models: { args: ["models"], out: /./ },
    workspace: { args: ["workspace"], out: /./ },
    pr: { args: ["pr"], out: /./ },
    doctor: { args: ["doctor"], out: /./ },
    completion: { args: ["completion", "bash"], out: /complete/ },
    bench: { args: ["bench", "silicon"], out: /./ },
    help: { args: ["help"], out: /Usage:/ },
    // Older subcommands dispatched in main.rs.
    status: { args: ["status"], out: /"product"/ },
    hardware: { args: ["hardware"], out: /./ },
    compute: { args: ["compute"], out: /./ },
    ggml: { args: ["ggml"], out: /rpcLoopback/ },
    openapi: { args: ["openapi"], out: /openapi/ },
    ui: { args: ["ui", "--out", join(OUT, "ui.png")], out: /./ },
    ostd: { args: ["ostd"], out: /./ },
    uutils: { args: ["uutils", "uname"], out: /./ },
    nix: { args: ["nix", "fw-e2e"], out: /./ },
    stack: { args: ["stack"], out: /./ },
    wayland: { args: ["wayland"], out: /./ },
    claw: { args: ["claw", "ranking"], out: /./ },
    migrate: { args: ["migrate", "bun"], out: /./ },
    // Fast paths.
    ls: { args: ["ls", "."], out: /a\.txt/ },
    table: { args: ["table", ".", "--json"], out: /a\.txt/ },
    find: { args: ["find", "a.txt", "."], out: /a\.txt/ },
    grep: { args: ["grep", "hello", "."], out: /hello/ },
  };
}

async function cliSection(): Promise<void> {
  seedWorkspace();
  mkdirSync(join(OUT, "project"), { recursive: true });
  const cases = cliCases();
  const commands = await helpCommands();
  if (commands.length === 0) record("cli", "discover", "fw --help", "fail", 0, "no commands found in fw --help");

  // 1. Version and help for every command and alias.
  await runCli("--version", { args: ["--version"], out: /^fw \d/ });
  await runCli("--help", { args: ["--help"], out: /Usage:/ });
  for (const { name, aliases } of commands) {
    for (const n of [name, ...aliases]) await runCli(`help ${n}`, { args: ["help", n], out: /Usage:/ });
    await runCli(`${name} --help`, { args: [name, "--help"], out: /Usage:/ });
  }
  await runCli("help (unknown)", { args: ["help", "no-such-command"], out: /Usage:/ });
  await runCli("unknown command", { args: ["no-such-command"], exit: "fail", err: /unknown command/ });
  for (const n of LEGACY) await runCli(`help ${n}`, { args: ["help", n], out: /./ });

  // 2. One real invocation per command, in dependency order (self-install before self-uninstall, mem after add).
  const order = [...commands.map((c) => c.name), "cluster k8s", ...LEGACY, "ls", "table", "find", "grep"];
  const missing = order.filter((n) => !cases[n]);
  for (const n of missing) record("cli", `run ${n}`, `fw ${n}`, "fail", 0, "no e2e case for this command");
  for (const n of order) {
    const c = cases[n];
    if (!c) continue;
    if (n === "serve") {
      await runCli(`run ${n} (bad address)`, c);
      continue;
    }
    await runCli(`run ${n}`, c);
    if (n === "mem") {
      await runCli("run mem add", { args: ["mem", "add", "e2e note about fw"], out: /./ });
      await runCli("run mem search", { args: ["mem", "search", "e2e"], out: /e2e/ });
    }
    if (n === "config") {
      await runCli("run config get", { args: ["config", "get", "model"], out: /e2e-model/ });
      await runCli("run config list", { args: ["config", "list"], out: /./ });
    }
    if (n === "mcp") {
      await runCli("run mcp add", { args: ["mcp", "add", "e2e", "echo", "hi"], out: /./ });
      await runCli("run mcp remove", { args: ["mcp", "remove", "e2e"], out: /./ });
    }
  }

  // 3. Multicall: the binary under an applet name, as uutils dispatches on argv[0].
  mkdirSync(join(OUT, "multicall"), { recursive: true });
  for (const [applet, args, expect] of [
    ["uname", [], /./],
    ["echo", ["multicall"], /multicall/],
    ["cat", ["a.txt"], /hello fw/],
  ] as const) {
    const copy = join(OUT, "multicall", `${applet}${EXE}`);
    copyFileSync(FW, copy);
    const r = await runProcess([copy, ...args]);
    const note = r.timedOut ? "timeout" : r.code !== 0 ? `exit ${r.code}: ${short(r.err)}` : expect.test(r.out) ? undefined : `stdout lacks ${expect}`;
    record("cli", `multicall ${applet}`, `${applet}${EXE} ${args.join(" ")}`.trim(), note ? "fail" : "ok", r.ms, note ?? "");
  }
}

// ---- serve -----------------------------------------------------------------------------------------------------

async function freePort(): Promise<number> {
  const probe = Bun.serve({ port: 0, fetch: () => new Response("") });
  const port = probe.port ?? 0;
  await probe.stop(true);
  return port;
}

async function serveSection(): Promise<void> {
  const port = await freePort();
  const base = `http://127.0.0.1:${port}`;
  const child = spawn(FW, ["serve", `127.0.0.1:${port}`], { cwd: WORK, env: sandboxEnv(), stdio: ["ignore", "pipe", "pipe"], windowsHide: true });
  let log = "";
  child.stdout.on("data", (d) => (log += d));
  child.stderr.on("data", (d) => (log += d));
  const started = performance.now();
  let up = false;
  for (let i = 0; i < 100 && child.exitCode === null; i++) {
    try {
      if ((await fetch(`${base}/health`, { signal: AbortSignal.timeout(1000) })).ok) {
        up = true;
        break;
      }
    } catch {
      await Bun.sleep(100);
    }
  }
  record("serve", "start", `fw serve 127.0.0.1:${port}`, up ? "ok" : "fail", Math.round(performance.now() - started), up ? "" : short(log));
  if (!up) {
    child.kill();
    return;
  }
  const json = { "content-type": "application/json" };
  const rpc = (method: string, params: unknown = {}) => JSON.stringify({ jsonrpc: "2.0", id: 1, method, params });
  type Probe = { name: string; method: string; path: string; body?: string; status?: number | "4xx"; check?: (text: string) => string | undefined };
  const jsonHas = (...keys: string[]) => (text: string) => {
    try {
      const v = JSON.parse(text);
      const missing = keys.filter((k) => !(k in v));
      return missing.length ? `missing ${missing.join(",")}` : undefined;
    } catch {
      return `not JSON: ${short(text, 60)}`;
    }
  };
  const probes: Probe[] = [
    { name: "GET /", method: "GET", path: "/", check: (t) => (/<html/i.test(t) ? undefined : "no HTML") },
    { name: "GET /health", method: "GET", path: "/health", check: jsonHas("ok") },
    { name: "GET /hardware", method: "GET", path: "/hardware" },
    { name: "GET /openapi.json", method: "GET", path: "/openapi.json", check: jsonHas("openapi", "paths") },
    { name: "GET /.well-known/agent.json", method: "GET", path: "/.well-known/agent.json" },
    { name: "GET /webgpu", method: "GET", path: "/webgpu", check: (t) => (/<html/i.test(t) ? undefined : "no HTML") },
    { name: "GET /cluster", method: "GET", path: "/cluster" },
    { name: "GET /v1/models", method: "GET", path: "/v1/models", check: jsonHas("data") },
    { name: "GET /v1/fx/status", method: "GET", path: "/v1/fx/status" },
    { name: "GET /claw/status", method: "GET", path: "/claw/status" },
    { name: "POST /mcp initialize", method: "POST", path: "/mcp", body: rpc("initialize", { protocolVersion: "2024-11-05", capabilities: {}, clientInfo: { name: "e2e", version: "0" } }), check: jsonHas("result") },
    { name: "POST /mcp tools/list", method: "POST", path: "/mcp", body: rpc("tools/list"), check: jsonHas("result") },
    { name: "POST /acp", method: "POST", path: "/acp", body: rpc("initialize", { protocolVersion: 1 }) },
    { name: "POST /a2a", method: "POST", path: "/a2a", body: rpc("agent/getAuthenticatedExtendedCard") },
    { name: "POST /graphql", method: "POST", path: "/graphql", body: '{"query":"{ status }"}' },
    { name: "POST /cluster/route", method: "POST", path: "/cluster/route", body: '{"prompt":"hello"}' },
    { name: "POST /v1/chat/completions", method: "POST", path: "/v1/chat/completions", body: '{"model":"fw","messages":[{"role":"user","content":"hi"}]}', status: 503, check: (t) => (/credentials/i.test(t) ? undefined : `no credentials message: ${short(t, 60)}`) },
    { name: "POST /mcp malformed", method: "POST", path: "/mcp", body: "{not json", status: "4xx", check: jsonHas("error") },
    { name: "GET /no-such-route", method: "GET", path: "/no-such-route", status: 404 },
  ];
  for (const p of probes) {
    const t0 = performance.now();
    let note: string | undefined;
    try {
      const res = await fetch(base + p.path, { method: p.method, headers: p.body ? json : undefined, body: p.body, signal: AbortSignal.timeout(15_000) });
      const text = await res.text();
      const want = p.status ?? 200;
      const statusOk = want === "4xx" ? res.status >= 400 && res.status < 500 : res.status === want;
      note = statusOk ? p.check?.(text) : `status ${res.status}: ${short(text, 70)}`;
      if (!note && p.name === "POST /v1/chat/completions") note = undefined;
    } catch (e) {
      note = `request failed: ${e instanceof Error ? e.message : e}`;
    }
    record("serve", p.name, `${p.method} ${p.path}`, note ? "fail" : "ok", Math.round(performance.now() - t0), note ?? "");
  }
  if (Bun.which("curl")) {
    const r = await runProcess(["curl", "-sS", "-m", "10", `${base}/health`]);
    record("serve", "curl /health", `curl ${base}/health`.replace(String(port), "$PORT"), r.code === 0 && /ok/.test(r.out) ? "ok" : "fail", r.ms, short(r.err || r.out, 60));
  }
  const alive = child.exitCode === null && !/panicked at/.test(log);
  child.kill();
  record("serve", "stop", "kill", alive ? "ok" : "fail", 0, alive ? "" : `server died or panicked: ${short(log)}`);
}

// ---- MCP -------------------------------------------------------------------------------------------------------

class McpClient {
  private buffer = "";
  private pending = new Map<number, (m: Record<string, unknown>) => void>();
  private nextId = 1;
  readonly stderr: string[] = [];
  private child;

  constructor(cmd: string[], env: Record<string, string>, cwd: string) {
    this.child = spawn(cmd[0], cmd.slice(1), { cwd, env, stdio: ["pipe", "pipe", "pipe"], windowsHide: true });
    this.child.stdout.on("data", (d) => this.onData(String(d)));
    this.child.stderr.on("data", (d) => this.stderr.push(String(d)));
    this.child.stdin.on("error", () => {});
  }

  private onData(chunk: string): void {
    this.buffer += chunk;
    for (let nl = this.buffer.indexOf("\n"); nl >= 0; nl = this.buffer.indexOf("\n")) {
      const line = this.buffer.slice(0, nl).trim();
      this.buffer = this.buffer.slice(nl + 1);
      if (!line.startsWith("{")) continue;
      try {
        const msg = JSON.parse(line) as Record<string, unknown>;
        const id = typeof msg.id === "number" ? msg.id : -1;
        this.pending.get(id)?.(msg);
        this.pending.delete(id);
      } catch {
        // A non-JSON line on stdout is reported by the missing response.
      }
    }
  }

  request(method: string, params: unknown, timeoutMs = 60_000): Promise<Record<string, unknown>> {
    const id = this.nextId++;
    return new Promise((resolveMsg, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(new Error(`timeout after ${timeoutMs}ms`));
      }, timeoutMs);
      this.pending.set(id, (m) => {
        clearTimeout(timer);
        resolveMsg(m);
      });
      this.child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", id, method, params })}\n`);
    });
  }

  notify(method: string): void {
    this.child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", method })}\n`);
  }

  close(): void {
    this.child.stdin.end();
    this.child.kill();
  }
}

/** Temporary fw root: the sources the MCP server reads, so tools that write (target scan, vfs, brain) never touch the checkout. */
function mcpRoot(): string {
  const root = join(SANDBOX, "mcp-root");
  mkdirSync(root, { recursive: true });
  for (const entry of ["src", "fw.targets.json", "fw.soul.json", "package.json", "configs/heretic", "core/ts/neon", "plugins/aphrody/.claude-plugin", "plugins/aphrody/hooks.json", "plugins/aphrody/plugin.json"]) {
    const from = join(ROOT, entry);
    if (existsSync(from)) cpSync(from, join(root, entry), { recursive: true });
  }
  const abi = findAbi();
  if (abi) {
    mkdirSync(join(root, "target", "agy"), { recursive: true });
    copyFileSync(abi, join(root, "target", "agy", basename(abi)));
  }
  mkdirSync(join(HOME_DIR, ".gemini", "scripts"), { recursive: true });
  writeFileSync(join(HOME_DIR, ".gemini", "scripts", "learn.ts"), 'console.log(JSON.stringify({ ok: true, stub: "e2e", argv: process.argv.slice(2) }));\n');
  return root;
}

function basename(p: string): string {
  return p.split(/[\\/]/).pop() ?? p;
}

/** The C ABI library: --abi, then the places src/targets.ts searches in the checkout. */
function findAbi(): string | undefined {
  const lib = IS_WIN ? "fw_abi.dll" : process.platform === "darwin" ? "libfw_abi.dylib" : "libfw_abi.so";
  const candidates = [flag("--abi"), process.env.FW_ABI, ...["target/agy", "native/fw-abi/target/release", "native/fw-abi/target/debug", "dist"].map((d) => join(ROOT, d, lib))];
  return candidates.find((p): p is string => Boolean(p) && existsSync(p as string));
}

/** A 512+512 byte PE32+ x86-64 image whose entry point is `xor eax, eax; ret`: input for the PE and disassembly tools on any OS. */
function writeMinimalPe(path: string): void {
  const img = Buffer.alloc(0x400, 0x90);
  img.fill(0, 0, 0x200);
  img.write("MZ", 0, "latin1");
  img.writeUInt32LE(0x40, 0x3c);
  img.write("PE\0\0", 0x40, "latin1");
  img.writeUInt16LE(0x8664, 0x44); // machine x86-64
  img.writeUInt16LE(1, 0x46); // sections
  img.writeUInt16LE(0xf0, 0x54); // optional header size
  img.writeUInt16LE(0x22, 0x56); // executable, large address aware
  const opt = 0x58;
  img.writeUInt16LE(0x20b, opt); // PE32+
  img.writeUInt32LE(0x200, opt + 4); // size of code
  img.writeUInt32LE(0x1000, opt + 16); // entry point RVA
  img.writeUInt32LE(0x1000, opt + 20); // base of code
  img.writeBigUInt64LE(0x140000000n, opt + 24); // image base
  img.writeUInt32LE(0x1000, opt + 32); // section alignment
  img.writeUInt32LE(0x200, opt + 36); // file alignment
  img.writeUInt16LE(6, opt + 40); // OS version
  img.writeUInt16LE(6, opt + 48); // subsystem version
  img.writeUInt32LE(0x2000, opt + 56); // size of image
  img.writeUInt32LE(0x200, opt + 60); // size of headers
  img.writeUInt16LE(3, opt + 68); // console subsystem
  img.writeUInt32LE(16, opt + 108); // data directories
  const sec = opt + 0xf0;
  img.write(".text", sec, "latin1");
  img.writeUInt32LE(0x200, sec + 8); // virtual size
  img.writeUInt32LE(0x1000, sec + 12); // virtual address
  img.writeUInt32LE(0x200, sec + 16); // raw size
  img.writeUInt32LE(0x200, sec + 20); // raw offset
  img.writeUInt32LE(0x60000020, sec + 36); // code, executable, readable
  img.set([0x31, 0xc0, 0xc3], 0x200);
  writeFileSync(path, img);
}

type ToolCall = { name: string; args: Record<string, unknown>; expect: "ok" | "error"; text?: RegExp };

function mcpCalls(root: string): ToolCall[] {
  const peSample = join(OUT, "sample.exe");
  writeMinimalPe(peSample);
  // Without the C ABI library the three native tools must answer with a clean isError result, not crash.
  const abi: Pick<ToolCall, "expect" | "text"> = findAbi() ? { expect: "ok" } : { expect: "error", text: /abi not found/ };
  return [
    { name: "fw_status", args: {}, expect: "ok", text: /"product": "fw"/ },
    { name: "fw_ast_parse", args: { lang: "rust", file: join(WORK, "src", "lib.rs") }, ...abi },
    { name: "fw_pe_inspect", args: { path: peSample }, ...abi },
    { name: "fw_target_scan", args: {}, expect: "ok", text: /manifestPath/ },
    { name: "fw_upgrade", args: { mode: "check" }, expect: "ok" },
    { name: "fw_bench", args: { iterations: 1 }, expect: "ok" },
    { name: "fw_learn", args: { action: "status" }, expect: "ok", text: /e2e/ },
    { name: "fw_service", args: { action: "status" }, expect: "ok" },
    { name: "fw_bridge", args: { subsystem: "craft" }, expect: "ok" },
    { name: "fw_disasm", args: { action: "disasm", path: peSample, maxInsns: 4 }, ...abi },
    { name: "fw_vfs", args: { action: "mounts" }, expect: "ok" },
    { name: "fw_ai", args: { action: "router" }, expect: "ok", text: /providers/ },
    { name: "fw_tui_terminal", args: { action: "status" }, expect: "ok" },
  ];
}

async function mcpSection(): Promise<void> {
  seedWorkspace();
  const root = mcpRoot();
  const server = join(ROOT, "plugins", "aphrody", "mcp", "fw-mcp.ts");
  const client = new McpClient([process.execPath, "--no-install", "--no-env-file", server], sandboxEnv({ FW_ROOT: root }), root);
  const timed = async <T>(fn: () => Promise<T>): Promise<[T | undefined, number, string | undefined]> => {
    const t0 = performance.now();
    try {
      return [await fn(), Math.round(performance.now() - t0), undefined];
    } catch (e) {
      return [undefined, Math.round(performance.now() - t0), e instanceof Error ? e.message : String(e)];
    }
  };
  try {
    const [init, initMs, initErr] = await timed(() => client.request("initialize", { protocolVersion: "2024-11-05", capabilities: {}, clientInfo: { name: "fw-e2e", version: "0" } }, 30_000));
    const serverName = (init?.result as { serverInfo?: { name?: string } } | undefined)?.serverInfo?.name;
    record("mcp", "initialize", "initialize", init && !initErr && serverName ? "ok" : "fail", initMs, initErr ?? `server ${serverName}`);
    if (!init) return;
    client.notify("notifications/initialized");

    const [listed, listMs, listErr] = await timed(() => client.request("tools/list", {}));
    const tools = ((listed?.result as { tools?: { name: string; inputSchema?: unknown }[] } | undefined)?.tools ?? []);
    const expectedTools = ["fw_ai", "fw_ast_parse", "fw_bench", "fw_bridge", "fw_disasm", "fw_learn", "fw_pe_inspect", "fw_service", "fw_status", "fw_target_scan", "fw_tui_terminal", "fw_upgrade", "fw_vfs"];
    const names = tools.map((t) => t.name);
    const lacking = expectedTools.filter((n) => !names.includes(n));
    const schemaBad = tools.filter((t) => !t.inputSchema).map((t) => t.name);
    record("mcp", "tools/list", "tools/list", !listErr && lacking.length === 0 && schemaBad.length === 0 ? "ok" : "fail", listMs, listErr ?? (lacking.length ? `missing ${lacking.join(",")}` : `${tools.length} tools`));

    const [pong, pingMs, pingErr] = await timed(() => client.request("ping", {}));
    record("mcp", "ping", "ping", pong && !pingErr ? "ok" : "fail", pingMs, pingErr ?? "");
    const [unknown, unkMs] = await timed(() => client.request("no/such-method", {}));
    record("mcp", "unknown method", "no/such-method", (unknown?.error as { code?: number } | undefined)?.code === -32601 ? "ok" : "fail", unkMs, "expects JSON-RPC -32601");

    for (const call of mcpCalls(root)) {
      if (!names.includes(call.name)) {
        record("mcp", call.name, `tools/call ${call.name}`, "fail", 0, "not in tools/list");
        continue;
      }
      const [res, ms, err] = await timed(() => client.request("tools/call", { name: call.name, arguments: call.args }, 120_000));
      const result = res?.result as { content?: { text?: string }[]; isError?: boolean } | undefined;
      const text = result?.content?.map((c) => c.text ?? "").join("") ?? "";
      let note = err;
      if (!note && !result) note = `JSON-RPC error: ${short(JSON.stringify(res?.error))}`;
      if (!note && PANIC.test(text)) note = `panic: ${short(text)}`;
      if (!note) {
        const isError = Boolean(result?.isError);
        if (call.expect === "ok" && isError) note = `isError: ${short(text)}`;
        else if (call.expect === "error" && !isError) note = "expected a clean isError result";
        else if (call.text && !call.text.test(text)) note = `text lacks ${call.text}: ${short(text)}`;
      }
      record("mcp", call.name, `tools/call ${call.name} ${JSON.stringify(call.args)}`.replaceAll(SANDBOX, "$SANDBOX"), note ? "fail" : "ok", ms, note ?? (call.expect === "error" ? "clean isError" : short(text, 60)));
    }
    const [bad, badMs] = await timed(() => client.request("tools/call", { name: "fw_no_such_tool", arguments: {} }));
    const badResult = bad?.result as { isError?: boolean } | undefined;
    record("mcp", "unknown tool", "tools/call fw_no_such_tool", badResult?.isError || bad?.error ? "ok" : "fail", badMs, "expects isError or JSON-RPC error");
  } finally {
    client.close();
  }
}

// ---- hooks -----------------------------------------------------------------------------------------------------

const PLUGIN = join(ROOT, "plugins", "aphrody");

type HookCase = { name: string; script: string; args?: string[]; stdin: string; env?: Record<string, string>; check: (out: unknown, r: Run) => string | undefined };

const parseOut = (r: Run): unknown => {
  try {
    return JSON.parse(r.out);
  } catch {
    return undefined;
  }
};

async function hookCase(c: HookCase): Promise<void> {
  const r = await runProcess([process.execPath, "--no-install", "--no-env-file", join(PLUGIN, "hooks", c.script), ...(c.args ?? [])], {
    cwd: PLUGIN,
    env: sandboxEnv({ CLAUDE_PLUGIN_ROOT: PLUGIN, CLAUDE_PLUGIN_DATA: join(SANDBOX, "plugin-data"), CLAUDE_CONFIG_DIR: join(HOME_DIR, ".claude"), ...c.env }),
    stdin: c.stdin,
    timeoutMs: 15_000,
  });
  const out = parseOut(r);
  let note: string | undefined;
  if (r.timedOut) note = "timeout";
  else if (r.code !== 0) note = `exit ${r.code}: ${short(r.err)}`;
  else if (out === undefined) note = `stdout is not JSON: ${short(r.out)}`;
  else note = c.check(out, r);
  record("hooks", c.name, `bun hooks/${c.script} ${(c.args ?? []).join(" ")}`.trim(), note ? "fail" : "ok", r.ms, note ?? short(r.out, 70));
}

const claudeInput = (command: string, tool = "Bash") =>
  JSON.stringify({ session_id: "e2e", transcript_path: "", cwd: WORK, hook_event_name: "PreToolUse", tool_name: tool, tool_input: { command }, tool_use_id: "toolu_e2e" });

async function hooksSection(): Promise<void> {
  const hso = (o: unknown) => (o as { hookSpecificOutput?: Record<string, unknown> }).hookSpecificOutput;
  const cases: HookCase[] = [
    {
      name: "session-start agy",
      script: "session-start.ts",
      args: ["agy"],
      stdin: JSON.stringify({ conversationId: "e2e", workspacePaths: [WORK] }),
      check: (o) => (JSON.stringify(o).includes("systemMessage") ? undefined : "no injectSteps.systemMessage"),
    },
    {
      name: "session-start claude",
      script: "session-start.ts",
      args: ["claude"],
      stdin: JSON.stringify({ session_id: "e2e", cwd: WORK, hook_event_name: "SessionStart", source: "startup" }),
      check: (o) => (hso(o)?.hookEventName === "SessionStart" && String(hso(o)?.additionalContext).length > 20 ? undefined : "no additionalContext"),
    },
    {
      name: "auto-update (fresh stamp)",
      script: "auto-update.ts",
      stdin: JSON.stringify({ session_id: "e2e", hook_event_name: "SessionStart" }),
      env: { CLAUDE_PLUGIN_DATA: join(SANDBOX, "plugin-data-fresh") },
      check: (o) => (JSON.stringify(o) === "{}" ? undefined : `expected {}, got ${JSON.stringify(o)}`),
    },
    {
      name: "pre-tool-use npm install (rewrite)",
      script: "pre-tool-use.ts",
      args: ["claude"],
      stdin: claudeInput("npm install express"),
      check: (o) => {
        const updated = hso(o)?.updatedInput as { command?: string } | undefined;
        return hso(o)?.permissionDecision === "allow" && /\bbun\b/.test(updated?.command ?? "") ? undefined : `no Bun rewrite: ${JSON.stringify(o)}`;
      },
    },
    {
      name: "pre-tool-use ls (passthrough)",
      script: "pre-tool-use.ts",
      args: ["claude"],
      stdin: claudeInput("ls -la"),
      check: (o) => (JSON.stringify(o) === "{}" ? undefined : `expected {}, got ${JSON.stringify(o)}`),
    },
    {
      name: "pre-tool-use non-shell tool",
      script: "pre-tool-use.ts",
      args: ["claude"],
      stdin: JSON.stringify({ session_id: "e2e", cwd: WORK, hook_event_name: "PreToolUse", tool_name: "Read", tool_input: { file_path: join(WORK, "a.txt") } }),
      check: (o) => (JSON.stringify(o) === "{}" ? undefined : `expected {}, got ${JSON.stringify(o)}`),
    },
    {
      name: "pre-tool-use empty stdin",
      script: "pre-tool-use.ts",
      args: ["claude"],
      stdin: "",
      check: (o) => (JSON.stringify(o) === "{}" ? undefined : `expected {}, got ${JSON.stringify(o)}`),
    },
    {
      name: "pre-tool-use malformed stdin",
      script: "pre-tool-use.ts",
      args: ["claude"],
      stdin: "{not json",
      check: (o) => (JSON.stringify(o) === "{}" ? undefined : `expected {}, got ${JSON.stringify(o)}`),
    },
    {
      name: "pre-tool-use agy run_command",
      script: "pre-tool-use.ts",
      args: ["agy"],
      stdin: JSON.stringify({ conversationId: "e2e", workspacePaths: [WORK], toolCall: { id: "1", name: "run_command", args: { CommandLine: "pnpm add zod" } } }),
      check: (o) => ((o as { decision?: string }).decision === "allow" ? undefined : `no allow decision: ${JSON.stringify(o)}`),
    },
    {
      name: "pre-tool-use gemini",
      script: "pre-tool-use.ts",
      args: ["gemini"],
      stdin: JSON.stringify({ cwd: WORK, tool_name: "run_shell_command", tool_input: { command: "yarn add zod" } }),
      check: (o) => ((o as { decision?: string }).decision === "allow" ? undefined : `no allow decision: ${JSON.stringify(o)}`),
    },
  ];
  for (const c of cases) await hookCase(c);
  await autoUpdateStale();
  await hooksJsonCommands();
  await rewriteUnit();
}

/** A stale stamp spawns a detached child that must call `claude plugin marketplace update fw` and `claude plugin update aphrody@fw`. */
async function autoUpdateStale(): Promise<void> {
  const shimDir = join(SANDBOX, "shim");
  const logFile = join(SANDBOX, "claude-shim.log");
  mkdirSync(shimDir, { recursive: true });
  if (IS_WIN) writeFileSync(join(shimDir, "claude.cmd"), `@echo off\r\necho %*>>"${logFile}"\r\n`);
  else {
    writeFileSync(join(shimDir, "claude"), `#!/bin/sh\necho "$@" >> "${logFile}"\n`, { mode: 0o755 });
  }
  const dataDir = join(SANDBOX, "plugin-data-stale");
  const env = sandboxEnv({ CLAUDE_PLUGIN_DATA: dataDir, PATH: `${shimDir}${delimiter}${process.env.PATH ?? ""}` });
  const r = await runProcess([process.execPath, "--no-install", "--no-env-file", join(PLUGIN, "hooks", "auto-update.ts")], { cwd: PLUGIN, env, stdin: "{}", timeoutMs: 5000 });
  let note: string | undefined;
  if (r.code !== 0 || r.out.trim() !== "{}") note = `hook exit ${r.code}, stdout ${short(r.out)}`;
  else if (r.ms > 2500) note = `hook took ${r.ms}ms, over its 3 s budget`;
  const t0 = performance.now();
  let log = "";
  while (!note && performance.now() - t0 < 20_000) {
    log = existsSync(logFile) ? readFileSync(logFile, "utf8") : "";
    if (/plugin update aphrody@fw/.test(log)) break;
    await Bun.sleep(250);
  }
  if (!note && !/marketplace update fw/.test(log)) note = `detached update did not run the marketplace update: ${short(log)}`;
  if (!note && !/plugin update aphrody@fw/.test(log)) note = `detached update did not run the plugin update: ${short(log)}`;
  if (!note && !existsSync(join(dataDir, "update.stamp"))) note = "stamp file not written";
  record("hooks", "auto-update (stale stamp)", "bun hooks/auto-update.ts (claude shim on PATH)", note ? "fail" : "ok", r.ms, note ?? "detached child ran both claude commands");
}

/** The commands registered in .claude-plugin/hooks.json, run as written with ${CLAUDE_PLUGIN_ROOT} substituted. */
async function hooksJsonCommands(): Promise<void> {
  const cfg = JSON.parse(readFileSync(join(PLUGIN, ".claude-plugin", "hooks.json"), "utf8")) as { hooks: Record<string, { matcher?: string; hooks: { command: string; timeout?: number }[] }[]> };
  for (const [event, groups] of Object.entries(cfg.hooks)) {
    for (const group of groups) {
      for (const hook of group.hooks) {
        const command = hook.command.replaceAll("${CLAUDE_PLUGIN_ROOT}", PLUGIN.replaceAll("\\", "/"));
        const payload = event === "PreToolUse" ? claudeInput("npm install express") : JSON.stringify({ session_id: "e2e", cwd: WORK, hook_event_name: event });
        const shell = [Bun.which("sh") ?? Bun.which("bash") ?? "sh", "-c", command];
        const r = await runProcess(shell, { cwd: WORK, env: sandboxEnv({ CLAUDE_PLUGIN_ROOT: PLUGIN, CLAUDE_PLUGIN_DATA: join(SANDBOX, "plugin-data-json") }), stdin: payload, timeoutMs: (hook.timeout ?? 10) * 1000 });
        let note: string | undefined;
        if (r.timedOut) note = `over its ${hook.timeout}s timeout`;
        else if (r.code !== 0) note = `exit ${r.code}: ${short(r.err)}`;
        else if (parseOut(r) === undefined) note = `stdout is not JSON: ${short(r.out)}`;
        else if (hook.timeout && r.ms > hook.timeout * 1000 * 0.8) note = `${r.ms}ms is within 20% of the ${hook.timeout}s timeout`;
        record("hooks", `hooks.json ${event}${group.matcher ? `[${group.matcher}]` : ""}`, command.replaceAll(PLUGIN.replaceAll("\\", "/"), "$PLUGIN"), note ? "fail" : "ok", r.ms, note ?? short(r.out, 60));
      }
    }
  }
}

/** rewrite.ts and common.ts are libraries: import them in a throwaway script and check the contract. */
async function rewriteUnit(): Promise<void> {
  const script = join(SANDBOX, "rewrite-unit.ts");
  writeFileSync(
    script,
    `import { rewriteCommand } from ${JSON.stringify(join(PLUGIN, "hooks", "rewrite.ts").replaceAll("\\", "/"))};
import { bunCapabilities, detectHarness, isBunCheckout } from ${JSON.stringify(join(PLUGIN, "hooks", "common.ts").replaceAll("\\", "/"))};
const opts = { bunCheckout: false, aphrody: false };
const rewritten = ["npm install express", "npx tsc", "yarn add zod", "pnpm run build", "node index.js"].map((c) => [c, rewriteCommand(c, opts)?.command ?? null]);
const untouched = ["ls -la", "git status", "echo npm install"].map((c) => [c, rewriteCommand(c, opts)?.command ?? null]);
console.log(JSON.stringify({ rewritten, untouched, caps: bunCapabilities(), harness: detectHarness("agy"), checkout: isBunCheckout(${JSON.stringify(WORK.replaceAll("\\", "/"))}) }));
`,
  );
  const r = await runProcess([process.execPath, "--no-install", "--no-env-file", script], { cwd: PLUGIN, timeoutMs: 30_000 });
  let note: string | undefined;
  try {
    const v = JSON.parse(r.out) as { rewritten: [string, string | null][]; untouched: [string, string | null][]; caps: { uv: boolean; mcp: boolean }; harness: string; checkout: boolean };
    const noRewrite = v.rewritten.filter(([, out]) => !out).map(([c]) => c);
    const wrongly = v.untouched.filter(([, out]) => out).map(([c]) => c);
    if (noRewrite.length) note = `no rewrite for ${noRewrite.join(" | ")}`;
    else if (wrongly.length) note = `rewrote ${wrongly.join(" | ")}`;
    else if (typeof v.caps.uv !== "boolean" || typeof v.caps.mcp !== "boolean") note = "bunCapabilities() malformed";
    else if (v.harness !== "agy" || v.checkout) note = "detectHarness/isBunCheckout wrong";
  } catch {
    note = `unit script failed: ${short(r.err || r.out)}`;
  }
  record("hooks", "rewrite.ts + common.ts", "rewriteCommand / bunCapabilities / detectHarness", note ? "fail" : "ok", r.ms, note ?? "rewrites and passthroughs as specified");
}

// ---- upgrade ---------------------------------------------------------------------------------------------------

const sha256 = (path: string) => createHash("sha256").update(readFileSync(path)).digest("hex");

async function upgradeSection(): Promise<void> {
  await runCli("upgrade --check", {
    args: ["upgrade", "--check"],
    check: (r) => {
      try {
        const v = JSON.parse(r.out) as Record<string, unknown>;
        if (v.note) return `no release resolved: ${short(String(v.note))}`;
        if (!v.matchedAsset) return `no asset for this platform: ${short(r.out)}`;
        return typeof v.updateAvailable === "boolean" ? undefined : "updateAvailable missing";
      } catch {
        return `not JSON: ${short(r.out)}`;
      }
    },
    timeoutMs: 60_000,
  }, FW, "upgrade");
  await runCli("upgrade --check --offline", { args: ["--offline", "upgrade", "--check"], exit: "fail", err: /offline/ }, FW, "upgrade");
  await runCli("update --version no-such-tag", { args: ["update", "--check", "--version", "no-such-tag-e2e"], exit: "fail", err: /./, timeoutMs: 60_000 }, FW, "upgrade");

  // Real upgrade of a temporary copy: the installed binary is never replaced.
  const dir = join(SANDBOX, "upgrade");
  mkdirSync(dir, { recursive: true });
  const copy = join(dir, `fw${EXE}`);
  copyFileSync(FW, copy);
  const before = sha256(copy);
  const r = await runProcess([copy, "upgrade"], { env: sandboxEnv(), timeoutMs: 180_000 });
  let note: string | undefined;
  let report: Record<string, unknown> = {};
  try {
    report = JSON.parse(r.out) as Record<string, unknown>;
  } catch {
    note = `not JSON: ${short(r.err || r.out)}`;
  }
  const after = sha256(copy);
  if (!note && r.code !== 0) note = `exit ${r.code}: ${short(r.err)}`;
  else if (!note && report.updated !== true) note = `not updated: ${short(r.out)}`;
  else if (!note && report.checksumVerified !== true) note = "SHA-256 not verified against the release checksums";
  else if (!note && after !== report.sha256) note = `file sha256 ${after.slice(0, 12)} differs from the reported ${String(report.sha256).slice(0, 12)}`;
  else if (!note && after === before) note = "binary unchanged";
  else if (!note && !existsSync(join(dir, IS_WIN ? "fw.exe.prev" : "fw.prev"))) note = ".prev backup missing";
  record("upgrade", "upgrade (temp copy)", "<temp>/fw upgrade", note ? "fail" : "ok", r.ms, note ?? `${statSync(copy).size} bytes, ${report.newTag}`);
  if (!note) {
    // The published release can be older than this checkout, so the upgraded copy is only required to start and answer.
    await runCli("upgraded copy runs", { args: ["help"], out: /./ }, copy, "upgrade");
  }
}

// ---- main ------------------------------------------------------------------------------------------------------

function printTable(): void {
  const header = ["section", "command", "result", "ms", "note"];
  const label = (r: Row) => (r.section === "cli" || r.section === "upgrade" ? r.cmd : r.name);
  const clip = (text: string) => (text.length > 70 ? `${text.slice(0, 67)}...` : text);
  const body = rows.map((r) => [r.section, clip(label(r)), r.result, String(r.ms), clip(r.note)]);
  const width = header.map((h, i) => Math.max(h.length, ...body.map((b) => b[i].length)));
  const line = (cells: string[]) => cells.map((c, i) => (i === 3 ? c.padStart(width[i]) : c.padEnd(width[i]))).join(" | ");
  console.log(line(header));
  console.log(width.map((w) => "-".repeat(w)).join("-|-"));
  for (const b of body) console.log(line(b));
  const failed = rows.filter((r) => r.result === "fail").length;
  console.log(`\n${rows.length - failed}/${rows.length} ok, ${failed} failed, total ${rows.reduce((a, r) => a + r.ms, 0)} ms, fw ${FW}, sandbox ${SANDBOX}`);
}

async function main(): Promise<number> {
  const steps: [Section, () => Promise<void>][] = [
    ["cli", cliSection],
    ["serve", serveSection],
    ["mcp", mcpSection],
    ["hooks", hooksSection],
    ["upgrade", upgradeSection],
  ];
  for (const [section, fn] of steps) {
    if (!wanted(section)) continue;
    if (FAST && (section === "upgrade" || section === "mcp")) continue;
    try {
      await fn();
    } catch (e) {
      record(section, "section", section, "fail", 0, e instanceof Error ? e.message : String(e));
    }
  }
  if (JSON_OUT) console.log(JSON.stringify({ fw: FW, root: ROOT, rows }, null, 2));
  else printTable();
  if (!argv.includes("--keep")) rmSync(SANDBOX, { recursive: true, force: true });
  return rows.some((r) => r.result === "fail") ? 1 : 0;
}

process.exit(await main());
