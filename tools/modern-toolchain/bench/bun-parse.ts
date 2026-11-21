#!/usr/bin/env bun
// Bun built-in parsers over the tracked files of this repository: best of 7 runs per parser.
//   bun tools/modern-toolchain/bench/bun-parse.ts [--jsonl path]
import { join } from "node:path";

const root = join(import.meta.dir, "..", "..", "..");
const tracked = (await Bun.$`git -C ${root} ls-files`.text()).split("\n").filter(Boolean);
const read = async (suffixes: string[]) => {
  const texts: string[] = [];
  for (const path of tracked.filter((entry) => suffixes.some((suffix) => entry.endsWith(suffix)))) texts.push(await Bun.file(join(root, path)).text().catch(() => ""));
  return texts;
};
const best = (work: () => unknown) => {
  let fastest = Infinity;
  for (let run = 0; run < 7; run++) {
    const start = performance.now();
    work();
    fastest = Math.min(fastest, performance.now() - start);
  }
  return fastest;
};
const attempt = (parse: (text: string) => unknown) => (text: string) => {
  try {
    parse(text);
    return 1;
  } catch {
    return 0;
  }
};
const row = (name: string, texts: string[], parse: (text: string) => unknown) => {
  const bytes = texts.reduce((sum, text) => sum + text.length, 0);
  const guarded = attempt(parse);
  const parsed = texts.reduce((sum, text) => sum + guarded(text), 0);
  const ms = best(() => texts.forEach(guarded));
  console.log(`${name.padEnd(28)} ${ms.toFixed(1).padStart(8)} ms ${(bytes / 1e6 / (ms / 1000)).toFixed(1).padStart(8)} MB/s parsed ${parsed}/${texts.length} (${bytes} chars)`);
};

row("Bun.YAML.parse", await read([".yml", ".yaml"]), (text) => Bun.YAML.parse(text));
row("Bun.TOML.parse", await read(["Cargo.toml", "Cargo.lock", "steps.toml", "build.toml"]), (text) => Bun.TOML.parse(text));
const flag = process.argv.indexOf("--jsonl");
if (flag > 0) {
  const lines = (await Bun.file(process.argv[flag + 1]).text()).split("\n").filter(Boolean);
  row("JSON.parse per line", lines, (text) => JSON.parse(text));
  const whole = `[${lines.join(",")}]`;
  row("JSON.parse whole array", [whole], (text) => JSON.parse(text));
}
