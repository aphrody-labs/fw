// Evaluates C integer constant expressions with the compiler: every expression becomes one line
// of a probe translation unit, expressions clang rejects are removed (their lines are named in
// the diagnostics), and the survivors are read back from the assembly clang emits.
import { mkdir, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { parallelMap, run, runOk } from "./exec.ts";

export interface Probe {
  key: string;
  expr: string;
}

export interface Evaluated {
  value: bigint;
  // True when the expression has a signed type and is negative.
  negative: boolean;
}

const CHUNK = 1500;

function probeLine(index: number, expr: string): string {
  return (
    `_Static_assert(_Generic((${expr}), float: 0, double: 0, long double: 0, default: 1), "float");` +
    `const struct { unsigned long long v; unsigned char n; } e_${index} = { (unsigned long long)(${expr}), (${expr}) < 0 };\n`
  );
}

async function evaluateChunk(cflags: string[], include: string, probes: Probe[], base: number, dir: string): Promise<Map<string, Evaluated>> {
  const file = join(dir, `probe-${base}.c`);
  let alive = probes.map((probe, offset) => ({ probe, index: base + offset }));
  const render = () => `#include "${include}"\n` + alive.map(({ probe, index }) => probeLine(index, probe.expr)).join("");
  for (let round = 0; round < 8; round++) {
    await writeFile(file, render());
    const check = await run(["clang", ...cflags, "-fsyntax-only", "-ferror-limit=0", file]);
    if (check.code === 0) break;
    const bad = new Set<number>();
    for (const match of check.stderr.matchAll(/probe-\d+\.c:(\d+):\d+: (?:fatal )?error/g)) bad.add(Number(match[1]) - 2);
    const before = alive.length;
    alive = alive.filter((_, position) => !bad.has(position));
    if (alive.length === before) {
      // Diagnostics could not be attributed to lines: fall back to one compile per expression.
      const verdicts = await parallelMap(alive, async ({ probe, index }) => {
        await writeFile(join(dir, `single-${index}.c`), `#include "${include}"\n${probeLine(index, probe.expr)}`);
        return (await run(["clang", ...cflags, "-fsyntax-only", join(dir, `single-${index}.c`)])).code === 0;
      }, 4);
      alive = alive.filter((_, position) => verdicts[position]);
    }
  }
  await writeFile(file, render());
  const asm = (await runOk(["clang", ...cflags, "-S", "-O0", "-o", "-", file])).stdout;
  const results = new Map<string, Evaluated>();
  const keyByIndex = new Map(alive.map(({ probe, index }) => [index, probe.key]));
  const lines = asm.split("\n");
  for (let i = 0; i < lines.length; i++) {
    const label = /^e_(\d+):/.exec(lines[i] as string);
    if (!label) continue;
    const key = keyByIndex.get(Number(label[1]));
    if (key === undefined) continue;
    let value: bigint | undefined;
    let negative: boolean | undefined;
    for (let j = i + 1; j < lines.length && !/^[\w.$]+:/.test(lines[j] as string); j++) {
      const text = lines[j] as string;
      const word = /^\s*\.(?:quad|xword)\s+(-?\d+)\s*(?:[#/].*)?$/.exec(text);
      if (word) value = BigInt.asUintN(64, BigInt(word[1] as string));
      const flag = /^\s*\.byte\s+(\d+)/.exec(text);
      if (flag && value !== undefined && negative === undefined) negative = flag[1] === "1";
      if (/^\s*\.zero\s+16/.test(text) && value === undefined) {
        value = 0n;
        negative = false;
      }
    }
    if (value !== undefined && negative !== undefined) results.set(key, { value, negative });
  }
  return results;
}

export async function evaluate(cflags: string[], include: string, probes: Probe[], workDir: string, tag: string): Promise<Map<string, Evaluated>> {
  const dir = join(workDir, `eval-${tag}`);
  await mkdir(dir, { recursive: true });
  const chunks: { probes: Probe[]; base: number }[] = [];
  for (let base = 0; base < probes.length; base += CHUNK) chunks.push({ probes: probes.slice(base, base + CHUNK), base });
  const parts = await parallelMap(chunks, (chunk) => evaluateChunk(cflags, include, chunk.probes, chunk.base, dir));
  const merged = new Map<string, Evaluated>();
  for (const part of parts) for (const [key, value] of part) merged.set(key, value);
  return merged;
}

// The value as the C type it had: negative values are sign-extended from 64 bits.
export function signedValue(evaluated: Evaluated): bigint {
  return evaluated.negative ? BigInt.asIntN(64, evaluated.value) : evaluated.value;
}
