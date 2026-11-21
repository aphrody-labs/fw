// Ioctl requests: every macro built with _IO/_IOR/_IOW/_IOWR/_IOC, decoded into the fields of the
// request number, with the argument type taken from the macro and matched to a generated record.
// Legacy requests (plain numbers such as TIOCGWINSZ in asm/ioctls.h) carry no argument type in
// the headers; the few whose argument struct is documented in tty_ioctl(4) are listed in
// LEGACY_ARGUMENTS and flagged in the output.
import { isIoctlBody } from "./constants.ts";
import { evaluate } from "./evaluate.ts";
import { type MacroTable } from "./preprocess.ts";
import { type Model } from "./model.ts";
import { type Tu } from "./tu.ts";

export interface IoctlDef {
  name: string;
  request: number;
  dir: number;
  type: number;
  nr: number;
  size: number;
  argType: string;
  // Key of the record the argument resolves to, when the macro names a struct or union and the
  // record size equals the size field of the request.
  recordKey: string | null;
  legacy: boolean;
}

export interface IoctlResult {
  ioctls: IoctlDef[];
  names: Set<string>;
  sizeMismatches: number;
  unresolvedArguments: number;
}

const LEGACY_FILES = new Set(["asm/ioctls.h", "asm-generic/ioctls.h"]);

// tty_ioctl(4): the argument of these legacy requests.
export const LEGACY_ARGUMENTS: Record<string, string> = {
  TCGETS: "struct termios",
  TCSETS: "struct termios",
  TCSETSW: "struct termios",
  TCSETSF: "struct termios",
  TCGETA: "struct termio",
  TCSETA: "struct termio",
  TCSETAW: "struct termio",
  TCSETAF: "struct termio",
  TIOCGWINSZ: "struct winsize",
  TIOCSWINSZ: "struct winsize",
};

function splitTopLevel(text: string): string[] {
  const parts: string[] = [];
  let depth = 0;
  let current = "";
  for (const char of text) {
    if (char === "(" || char === "[") depth++;
    if (char === ")" || char === "]") depth--;
    if (char === "," && depth === 0) {
      parts.push(current.trim());
      current = "";
    } else current += char;
  }
  parts.push(current.trim());
  return parts;
}

// The C type the macro names: the third argument of _IOx(type, nr, argtype), or the operand of
// sizeof in _IOC(dir, type, nr, sizeof(argtype)). "" when the macro does not name one.
function argumentOf(body: string): string {
  const call = /\b(_IO(?:R|W|WR)?(?:_BAD)?|_IOC)\s*\(/.exec(body);
  if (!call) return "";
  const start = (call.index ?? 0) + call[0].length;
  let depth = 1;
  let end = start;
  while (end < body.length && depth > 0) {
    if (body[end] === "(") depth++;
    if (body[end] === ")") depth--;
    end++;
  }
  const args = splitTopLevel(body.slice(start, end - 1));
  if (call[1] === "_IO") return "";
  if (call[1] === "_IOC") {
    const size = /^sizeof\s*\(\s*([^()]*(?:\([^()]*\))?[^()]*)\)\s*$/.exec(args[3] ?? "");
    return size ? (size[1] as string).trim() : "";
  }
  return (args[2] ?? "").replace(/\s+/g, " ").trim();
}

function recordKeyFor(argType: string, model: Model): string | null {
  const tagged = /^(struct|union)\s+(\w+)$/.exec(argType);
  if (tagged) {
    const key = `${tagged[1]}_${tagged[2]}`;
    return model.records.has(key) ? key : null;
  }
  const alias = model.aliases.get(`typedef_${argType}`);
  if (alias && model.records.has(alias)) return alias;
  if (model.records.has(`typedef_${argType}`)) return `typedef_${argType}`;
  return null;
}

export async function collectIoctls(tu: Tu, macros: MacroTable, evaluated: Map<string, bigint>, model: Model): Promise<IoctlResult> {
  const named = new Map<string, string>();
  for (const macro of macros.objectLike.values()) {
    if (!evaluated.has(macro.name)) continue;
    if (isIoctlBody(macro.body)) named.set(macro.name, macro.body);
  }
  // Aliases of ioctl macros (#define OLD NEW).
  for (const macro of macros.objectLike.values()) {
    if (named.has(macro.name) || !evaluated.has(macro.name)) continue;
    if (/^\w+$/.test(macro.body) && named.has(macro.body)) named.set(macro.name, named.get(macro.body) as string);
  }
  const legacy = new Set<string>();
  for (const macro of macros.objectLike.values()) {
    if (LEGACY_FILES.has(macro.file) && evaluated.has(macro.name) && !named.has(macro.name) && /^(?:TC|TIOC|FIO)[A-Z0-9]+$/.test(macro.name)) legacy.add(macro.name);
  }

  const layoutProbes = [
    "_IOC_NRSHIFT", "_IOC_TYPESHIFT", "_IOC_SIZESHIFT", "_IOC_DIRSHIFT", "_IOC_NRBITS", "_IOC_TYPEBITS", "_IOC_SIZEBITS", "_IOC_DIRBITS", "_IOC_NONE", "_IOC_WRITE", "_IOC_READ",
  ];
  const fields = await evaluate(tu.cflags, tu.sourcePath, layoutProbes.map((name) => ({ key: name, expr: name })), tu.workDir, "ioc");
  const field = (name: string): bigint => {
    const result = fields.get(name);
    if (!result) throw new Error(`${name} is not available: ioctl numbers cannot be decoded`);
    return result.value;
  };
  if (field("_IOC_NONE") !== 0n || field("_IOC_WRITE") !== 1n || field("_IOC_READ") !== 2n) throw new Error("this arch does not use the asm-generic _IOC direction encoding");
  const bits = { nr: field("_IOC_NRBITS"), type: field("_IOC_TYPEBITS"), size: field("_IOC_SIZEBITS"), dir: field("_IOC_DIRBITS") };
  const shift = { nr: field("_IOC_NRSHIFT"), type: field("_IOC_TYPESHIFT"), size: field("_IOC_SIZESHIFT"), dir: field("_IOC_DIRSHIFT") };
  const extract = (request: bigint, name: keyof typeof bits): number => Number((request >> shift[name]) & ((1n << bits[name]) - 1n));

  const ioctls: IoctlDef[] = [];
  let sizeMismatches = 0;
  let unresolvedArguments = 0;
  const all = [...named.keys(), ...legacy].sort();
  for (const name of all) {
    const value = evaluated.get(name) as bigint;
    if (value < 0n || value > 0xffffffffn) continue;
    const isLegacy = legacy.has(name);
    const argType = isLegacy ? (LEGACY_ARGUMENTS[name] ?? "") : argumentOf(named.get(name) as string);
    const size = isLegacy ? 0 : extract(value, "size");
    let recordKey = argType ? recordKeyFor(argType, model) : null;
    if (argType && !recordKey && /^(struct|union)\s/.test(argType)) unresolvedArguments++;
    if (recordKey && !isLegacy && (model.records.get(recordKey) as { size: number }).size !== size) {
      sizeMismatches++;
      recordKey = null;
    }
    ioctls.push({
      name,
      request: Number(value),
      dir: isLegacy ? 0 : extract(value, "dir"),
      type: isLegacy ? Number((value >> 8n) & 0xffn) : extract(value, "type"),
      nr: isLegacy ? Number(value & 0xffn) : extract(value, "nr"),
      size,
      argType,
      recordKey,
      legacy: isLegacy,
    });
  }
  return { ioctls, names: new Set(all), sizeMismatches, unresolvedArguments };
}
