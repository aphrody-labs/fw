// Reads the macro table of a translation unit with `clang -E -dD`, keeping the header each
// definition came from and resolving definitions that disagree between headers.
import { run } from "./exec.ts";
import { VARIANT } from "./tu.ts";

export interface MacroDef {
  name: string;
  body: string;
  file: string;
}

export interface MacroTable {
  objectLike: Map<string, MacroDef>;
  // Object-like macros defined with different bodies by headers that are all primary (not
  // arch-variant or asm-generic fallbacks): ambiguous, so not part of the table.
  ambiguous: Map<string, MacroDef[]>;
}

export async function readMacros(cflags: string[], source: string, includeDir: string, stubDir: string): Promise<MacroTable> {
  const result = await run(["clang", ...cflags, "-E", "-dD", source]);
  if (result.code !== 0) throw new Error(`preprocess failed: ${result.stderr.slice(0, 2000)}`);
  const history = new Map<string, MacroDef[]>();
  let file = "";
  for (const line of result.stdout.split("\n")) {
    if (line.startsWith("# ")) {
      const marker = /^# \d+ "([^"]*)"/.exec(line);
      if (marker) file = marker[1] as string;
      continue;
    }
    if (!line.startsWith("#")) continue;
    const undef = /^#undef\s+(\w+)/.exec(line);
    if (undef) {
      history.delete(undef[1] as string);
      continue;
    }
    const define = /^#define\s+(\w+)(\()?\s*(.*)$/.exec(line);
    if (!define) continue;
    const name = define[1] as string;
    let origin: string;
    if (file.startsWith(includeDir)) origin = file.slice(includeDir.length + 1);
    else if (file.startsWith(stubDir)) origin = `stub:${file.slice(stubDir.length + 1)}`;
    else continue;
    // A function-like definition shadows any object-like one of the same name.
    const body = (define[3] as string).trim();
    const list = history.get(name) ?? [];
    if (define[2]) {
      history.set(name, [{ name, body: "\0function-like", file: origin }]);
      continue;
    }
    if (!list.some((entry) => entry.body === body)) list.push({ name, body, file: origin });
    history.set(name, list);
  }
  const objectLike = new Map<string, MacroDef>();
  const ambiguous = new Map<string, MacroDef[]>();
  for (const [name, list] of history) {
    if (list.length === 1) {
      const only = list[0] as MacroDef;
      if (only.body !== "\0function-like") objectLike.set(name, only);
      continue;
    }
    const primary = list.filter((entry) => !VARIANT.test(entry.file));
    if (primary.length === 1) objectLike.set(name, primary[0] as MacroDef);
    else ambiguous.set(name, list);
  }
  return { objectLike, ambiguous };
}
