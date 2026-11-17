// The few system helpers the tool needs, on Bun's own APIs (Bun.file, Bun.$, Bun.Glob, navigator, crypto). No node:* module
// except `node:path`, whose string functions have no Bun equivalent.
import { join } from "node:path";

export const exists = (path: string): Promise<boolean> =>
  Bun.file(path)
    .stat()
    .then(
      () => true,
      () => false,
    );

export const isDirectory = (path: string): Promise<boolean> =>
  Bun.file(path)
    .stat()
    .then(
      (s) => s.isDirectory(),
      () => false,
    );

export const mkdirp = async (path: string) => void (await Bun.$`mkdir -p ${path}`.quiet());

/** Removes a file or a tree; a path that is not there, or briefly held (a browser still closing), is not an error. */
export const rmrf = async (path: string) => {
  for (let attempt = 0; attempt < 10; attempt++) {
    if ((await Bun.$`rm -rf ${path}`.nothrow().quiet()).exitCode === 0) return;
    await Bun.sleep(200);
  }
};

export const move = async (from: string, to: string) => void (await Bun.$`mv ${from} ${to}`.quiet());

/** Names (not paths) of the entries of a directory, files and directories alike. */
export const list = async (dir: string, pattern = "*"): Promise<string[]> =>
  (await exists(dir)) ? Array.fromAsync(new Bun.Glob(pattern).scan({ cwd: dir, onlyFiles: false })) : [];

export const tmpdir = () => Bun.env.TMPDIR ?? Bun.env.TEMP ?? Bun.env.TMP ?? "/tmp";

export async function mkdtemp(prefix: string): Promise<string> {
  const dir = join(tmpdir(), `${prefix}${crypto.randomUUID().slice(0, 8)}`);
  await mkdirp(dir);
  return dir;
}

export const hostname = async (): Promise<string> =>
  (Bun.env.HOSTNAME ?? Bun.env.COMPUTERNAME ?? (await Bun.$`hostname`.text())).trim();

export const cores = () => navigator.hardwareConcurrency;

/** 1, 5 and 15 minute load averages where the OS publishes them (Linux); empty elsewhere. */
export async function loadavg(): Promise<number[]> {
  const text = await Bun.file("/proc/loadavg")
    .text()
    .catch(() => "");
  return text.split(" ").slice(0, 3).map(Number).filter(Number.isFinite);
}

/** Resident memory of a process and its descendants in KiB (Linux procfs; null elsewhere). */
export async function treeRss(pid: number): Promise<number | null> {
  if (process.platform !== "linux") return null;
  let total = 0;
  const stack = [pid];
  const seen = new Set<number>();
  while (stack.length) {
    const p = stack.pop()!;
    if (seen.has(p)) continue;
    seen.add(p);
    try {
      total += Number((await Bun.file(`/proc/${p}/status`).text()).match(/^VmRSS:\s+(\d+)/m)?.[1] ?? 0);
      stack.push(...(await Bun.file(`/proc/${p}/task/${p}/children`).text()).split(" ").filter(Boolean).map(Number));
    } catch {} // the process left between two reads
  }
  return total;
}

type Spec = Record<string, "string" | "boolean" | "strings">;
/** `--name value`, `--name=value`, `--flag`, repeatable strings; everything else is positional. */
export function parseFlags<S extends Spec>(argv: string[], spec: S) {
  const values: Record<string, string | boolean | string[] | undefined> = {};
  const positionals: string[] = [];
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (!arg.startsWith("--")) {
      positionals.push(arg);
      continue;
    }
    const [name, inline] = arg.slice(2).split(/=(.*)/s);
    const kind = spec[name];
    if (!kind) throw new Error(`unknown flag --${name}`);
    if (kind === "boolean") values[name] = true;
    else {
      const value = inline ?? argv[++i];
      if (value === undefined) throw new Error(`--${name} needs a value`);
      if (kind === "strings") (values[name] ??= [] as string[]) && (values[name] as string[]).push(value);
      else values[name] = value;
    }
  }
  type Out = { [K in keyof S]?: S[K] extends "boolean" ? boolean : S[K] extends "strings" ? string[] : string };
  return { values: values as Out, positionals };
}

/** Samples the resident memory of a process tree every `every_ms`; `peak()` is the highest seen, in KiB. */
export function sampleRss(pid: number, every_ms: number) {
  let peak: number | null = null;
  let busy = false;
  const take = async () => {
    if (busy) return; // a slow read is not queued behind itself
    busy = true;
    const now = await treeRss(pid);
    if (now !== null) peak = Math.max(peak ?? 0, now);
    busy = false;
  };
  void take();
  const timer = setInterval(take, every_ms);
  return { peak: () => peak, stop: () => clearInterval(timer) };
}
