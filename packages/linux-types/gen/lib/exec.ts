import { availableParallelism } from "node:os";

export interface RunResult {
  code: number;
  stdout: string;
  stderr: string;
}

export async function run(cmd: string[], options: { cwd?: string; stdin?: string } = {}): Promise<RunResult> {
  const proc = Bun.spawn(cmd, {
    cwd: options.cwd,
    stdin: options.stdin === undefined ? "ignore" : new Blob([options.stdin]),
    stdout: "pipe",
    stderr: "pipe",
  });
  const [stdout, stderr, code] = await Promise.all([
    new Response(proc.stdout).text(),
    new Response(proc.stderr).text(),
    proc.exited,
  ]);
  return { code, stdout, stderr };
}

export async function runOk(cmd: string[], options: { cwd?: string; stdin?: string } = {}): Promise<RunResult> {
  const result = await run(cmd, options);
  if (result.code !== 0) {
    throw new Error(`${cmd.join(" ")} exited ${result.code}\n${result.stderr.slice(0, 4000)}`);
  }
  return result;
}

// Runs `worker` over `items` with at most `limit` in flight; results keep the input order.
export async function parallelMap<T, R>(items: T[], worker: (item: T) => Promise<R>, limit = availableParallelism()): Promise<R[]> {
  const results = new Array<R>(items.length);
  let next = 0;
  async function lane(): Promise<void> {
    for (;;) {
      const index = next++;
      if (index >= items.length) return;
      results[index] = await worker(items[index] as T);
    }
  }
  await Promise.all(Array.from({ length: Math.min(limit, items.length) }, lane));
  return results;
}

export class Timings {
  readonly stages: { stage: string; seconds: number }[] = [];
  async time<T>(stage: string, body: () => Promise<T>): Promise<T> {
    const start = performance.now();
    try {
      return await body();
    } finally {
      this.stages.push({ stage, seconds: (performance.now() - start) / 1000 });
    }
  }
}
