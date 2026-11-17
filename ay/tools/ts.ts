// The tree-sitter layer of the SDK (ADR-0008): one parser per language, loaded from the grammar packages' own wasm
// builds through web-tree-sitter, and the fact extractors the ABI tool needs (macros, constants, function
// signatures) from C, Rust and Python. Types are normalized to the ABI spec's vocabulary (u32, i32, u64, usize, *const T).
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { Language, Parser, type Node } from "web-tree-sitter";

export type Lang = "c" | "rust" | "python";
export type Sig = { ret: string; params: string[] };
export type Facts = { consts: Map<string, number>; fns: Map<string, Sig> };

const require_ = createRequire(import.meta.url);
let ready: Promise<void> | undefined;
const languages = new Map<Lang, Language>();

export async function parse(lang: Lang, source: string): Promise<Node> {
  await (ready ??= Parser.init());
  if (!languages.has(lang)) languages.set(lang, await Language.load(readFileSync(require_.resolve(`tree-sitter-${lang}/tree-sitter-${lang}.wasm`))));
  const parser = new Parser();
  parser.setLanguage(languages.get(lang)!);
  return parser.parse(source)!.rootNode;
}

/** Positions of every syntax error in a tree; empty when the source parses cleanly. */
export function syntaxErrors(root: Node): string[] {
  if (!root.hasError) return [];
  const out: string[] = [];
  const walk = (n: Node) => {
    if (n.type === "ERROR" || n.isMissing) out.push(`${n.startPosition.row + 1}:${n.startPosition.column + 1} ${n.isMissing ? `missing ${n.type}` : n.text.slice(0, 40)}`);
    else n.children.forEach((c) => c && walk(c));
  };
  walk(root);
  return out;
}

const C_TYPES: Record<string, string> = { uint8_t: "u8", uint16_t: "u16", uint32_t: "u32", uint64_t: "u64", int8_t: "i8", int16_t: "i16", int32_t: "i32", int64_t: "i64", size_t: "usize", uintptr_t: "usize", void: "void", bool: "bool" };

/** A C type as the spec writes it: `const uint32_t *` becomes `*const u32`. */
export function cType(base: string, qualifiers: string[], pointers: number): string {
  const name = C_TYPES[base.trim()] ?? base.trim();
  let out = name;
  for (let i = 0; i < pointers; i++) out = `*${i === 0 && qualifiers.includes("const") ? "const" : "mut"} ${out}`;
  return out;
}

const number = (text: string): number | undefined => {
  const t = text.replace(/\/\*.*?\*\/|\/\/.*$/g, "").replace(/[()\s]/g, "").replace(/[uUlL]+$/, "");
  return /^-?(0x[0-9a-f]+|\d+)$/i.test(t) ? Number(t) : undefined;
};

const qualifiersOf = (n: Node): string[] => n.children.filter((c) => c?.type === "type_qualifier").map((c) => c!.text);

function cSignature(n: Node): [string, Sig] | undefined {
  let declarator = n.childForFieldName("declarator");
  let pointers = 0;
  while (declarator?.type === "pointer_declarator") (pointers++, (declarator = declarator.childForFieldName("declarator")));
  if (declarator?.type !== "function_declarator") return undefined;
  const name = declarator.childForFieldName("declarator")?.text;
  if (!name) return undefined;
  const ret = cType(n.childForFieldName("type")?.text ?? "", qualifiersOf(n), pointers);
  const params: string[] = [];
  for (const p of declarator.childForFieldName("parameters")?.namedChildren ?? []) {
    if (p?.type !== "parameter_declaration") continue;
    let d = p.childForFieldName("declarator");
    let depth = 0;
    while (d?.type === "pointer_declarator") (depth++, (d = d.childForFieldName("declarator")));
    const type = cType(p.childForFieldName("type")?.text ?? "", qualifiersOf(p), depth);
    if (type !== "void") params.push(type);
  }
  return [name, { ret, params }];
}

export function cFacts(root: Node): Facts {
  const consts = new Map<string, number>();
  for (const d of root.descendantsOfType("preproc_def")) {
    const name = d?.childForFieldName("name")?.text;
    const value = number(d?.childForFieldName("value")?.text ?? "");
    if (name && value !== undefined) consts.set(name, value);
  }
  const fns = new Map<string, Sig>();
  for (const n of root.descendantsOfType(["declaration", "function_definition"])) {
    const sig = n && cSignature(n);
    if (sig) fns.set(sig[0], sig[1]);
  }
  return { consts, fns };
}

/** Case value to returned number of a switch-with-returns function (the corelib's ay_arg_count); `default` is -1. */
export function switchTable(root: Node, fn: string, names: Map<string, number>): Map<number, number> | undefined {
  const def = root.descendantsOfType("function_definition").find((d) => d && cSignature(d)?.[0] === fn);
  if (!def) return undefined;
  const table = new Map<number, number>();
  let pending: number[] = [];
  for (const c of def.descendantsOfType("case_statement")) {
    if (!c) continue;
    const value = c.childForFieldName("value");
    pending.push(value ? (names.get(value.text) ?? number(value.text) ?? NaN) : -1);
    const ret = c.namedChildren.find((x) => x?.type === "return_statement");
    if (ret) {
      const result = number(ret.namedChildren[0]?.text ?? "");
      for (const label of pending) table.set(label, result ?? NaN);
      pending = [];
    }
  }
  return table;
}

const rustType = (t: string) => t.replace(/\s+/g, " ").trim();

export function rustFacts(root: Node): Facts {
  const consts = new Map<string, number>();
  for (const c of root.descendantsOfType("const_item")) {
    const name = c?.childForFieldName("name")?.text;
    const value = number(c?.childForFieldName("value")?.text ?? "");
    if (name && value !== undefined) consts.set(name, value);
  }
  const fns = new Map<string, Sig>();
  // Declarations in extern blocks, and definitions exported with the C calling convention.
  const items = [...root.descendantsOfType("function_signature_item"), ...root.descendantsOfType("function_item").filter((f) => f?.descendantsOfType("extern_modifier").length)];
  for (const f of items) {
    const name = f?.childForFieldName("name")?.text;
    if (!name) continue;
    const params = (f.childForFieldName("parameters")?.namedChildren ?? []).filter((p) => p?.type === "parameter").map((p) => rustType(p!.childForFieldName("type")?.text ?? ""));
    fns.set(name, { ret: rustType(f.childForFieldName("return_type")?.text ?? "void"), params });
  }
  return { consts, fns };
}

/** Module-level integer assignments and the arity of each entry of a `SIGNATURES` dict. */
export function pythonFacts(root: Node): { consts: Map<string, number>; arity: Map<string, number> } {
  const consts = new Map<string, number>();
  const arity = new Map<string, number>();
  for (const s of root.namedChildren) {
    const a = s?.type === "expression_statement" ? s.namedChildren[0] : undefined;
    if (a?.type !== "assignment") continue;
    const left = a.childForFieldName("left")?.text;
    const right = a.childForFieldName("right");
    if (!left || !right) continue;
    if (left === "SIGNATURES") {
      for (const pair of right.descendantsOfType("pair")) {
        const key = pair?.childForFieldName("key")?.text.replace(/^["']|["']$/g, "");
        const value = pair?.childForFieldName("value");
        const list = value?.namedChildren[1];
        if (key && list) arity.set(key, list.namedChildren.length);
      }
    } else {
      const v = number(right.text);
      if (v !== undefined) consts.set(left, v);
    }
  }
  return { consts, arity };
}
