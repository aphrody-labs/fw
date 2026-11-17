// The few system helpers the tool needs, on Bun's own APIs (Bun.file, Bun.$, Bun.Glob, navigator, crypto). No node:* module
// except `node:path`, whose string functions have no Bun equivalent.
import { hostname, loadavg, tmpdir } from "node:os";
import { join } from "node:path";

export { hostname, loadavg, tmpdir };

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

export async function mkdtemp(prefix: string): Promise<string> {
  const dir = join(tmpdir(), `${prefix}${crypto.randomUUID().slice(0, 8)}`);
  await mkdirp(dir);
  return dir;
}

export const cores = () => navigator.hardwareConcurrency;

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
    } catch (error) {
      if ((error as { code?: string }).code !== "ENOENT") throw error; // the process left between two reads
    }
  }
  return total;
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

/** Bun.spawn that never opens a console window on Windows (docs/runtime/child-process.mdx, windowsHide). */
export const spawn = ((cmd: string[], options: object = {}) =>
  Bun.spawn(cmd, { windowsHide: true, ...options })) as typeof Bun.spawn;
