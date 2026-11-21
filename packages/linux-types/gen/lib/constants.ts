// Integer constants: object-like macros evaluated by the compiler plus enumerators from BTF,
// merged into one namespace. Ioctl requests and syscall numbers are classified out and go to
// their own tables.
import { evaluate, signedValue } from "./evaluate.ts";
import { type MacroTable } from "./preprocess.ts";
import { type Model } from "./model.ts";
import { type Tu } from "./tu.ts";

export interface Constant {
  name: string;
  value: bigint;
  // Header the macro was defined in, or "" for an enumerator.
  header: string;
  // For enumerators, the enum they belong to ("(anon)" for anonymous ones).
  enumName: string;
}

export interface MacroEvaluation {
  evaluated: Map<string, bigint>;
  candidates: number;
  notConstants: number;
}

const IOCTL_BODY = /\b_IO(?:R|W|WR)?(?:_BAD)?\s*\(|\b_IOC\s*\(/;

export function isIoctlBody(body: string): boolean {
  return IOCTL_BODY.test(body);
}

export async function evaluateMacros(tu: Tu, macros: MacroTable): Promise<MacroEvaluation> {
  const candidates = [...macros.objectLike.values()].filter((macro) => macro.body !== "" && !macro.file.startsWith("stub:") && !macro.name.startsWith("__NR_"));
  const results = await evaluate(tu.cflags, tu.sourcePath, candidates.map((macro) => ({ key: macro.name, expr: macro.name })), tu.workDir, "macros");
  const evaluated = new Map<string, bigint>();
  for (const macro of candidates) {
    const result = results.get(macro.name);
    if (result) evaluated.set(macro.name, signedValue(result));
  }
  return { evaluated, candidates: candidates.length, notConstants: candidates.length - evaluated.size };
}

const RESERVED = new Set(
  "break case catch class const continue debugger default delete do else enum export extends false finally for function if import in instanceof new null return super switch this throw true try typeof var void while with yield let static implements interface package private protected public await async eval arguments".split(" "),
);

export function checkIdentifier(name: string, what: string): void {
  if (!/^[A-Za-z_$][A-Za-z0-9_$]*$/.test(name) || RESERVED.has(name)) throw new Error(`${what} ${JSON.stringify(name)} is not usable as a TypeScript export name`);
}

export interface MergedConstants {
  constants: Constant[];
  // Macros that redefine an enumerator's name to a different value (`#define X (X - 1)`): the
  // enumerator, which is what the enum declares, is kept.
  shadowedMacros: string[];
}

// Merges macro constants (minus the excluded names) with enumerators. Two enumerators of one name
// with different values fail the run.
export function mergeConstants(model: Model, macros: MacroTable, evaluated: Map<string, bigint>, excluded: Set<string>): MergedConstants {
  const shadowedMacros: string[] = [];
  const byName = new Map<string, Constant>();
  for (const enumerator of model.enumerators) {
    const previous = byName.get(enumerator.name);
    if (previous) {
      if (previous.value !== enumerator.value) throw new Error(`enumerator ${enumerator.name} has two values: ${previous.value} and ${enumerator.value}`);
      continue;
    }
    byName.set(enumerator.name, { name: enumerator.name, value: enumerator.value, header: "", enumName: enumerator.enumCname });
  }
  for (const [name, value] of evaluated) {
    if (excluded.has(name)) continue;
    const macro = macros.objectLike.get(name);
    const previous = byName.get(name);
    if (previous) {
      if (previous.value !== value) shadowedMacros.push(name);
      continue;
    }
    byName.set(name, { name, value, header: macro?.file ?? "", enumName: "" });
  }
  for (const name of byName.keys()) checkIdentifier(name, "constant");
  return { constants: [...byName.values()], shadowedMacros };
}
