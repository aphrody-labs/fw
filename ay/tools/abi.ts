#!/usr/bin/env bun
// One spec, every binding. abi/ay.abi.toml is the source of truth of the SDK ABI; this tool generates from it
//   corelib/include/<name>_abi.h   the C header (the stable boundary of ADR-0008)
//   abi/gen/<name>_abi.rs          Rust constants and `extern "C"` declarations
//   abi/gen/<name>_abi.ts          Bun constants and the `bun:ffi` symbol table
//   abi/gen/<name>_abi.py          Python constants and ctypes signatures
//   abi/gen/<name>_abi.json        the machine-readable form, with the spec's hash
//   docs/abi.md                    the human spec
// and checks with tree-sitter (tools/ts.ts) that every generated file parses cleanly and says what the spec says, and
// that the corelib sources define exactly the spec's functions with the same signatures and syscall arities.
//   bun tools/abi.ts           write the generated files, then check
//   bun tools/abi.ts --check   write nothing; fail when a file is stale or a check fails
import { existsSync, readdirSync } from "node:fs";
import { join, resolve } from "node:path";
import { cFacts, parse, pythonFacts, rustFacts, switchTable, syntaxErrors, type Sig } from "./ts.ts";

const root = resolve(import.meta.dir, "..");

export type Const = { name: string; value: number; doc?: string; nargs?: number };
export type Group = { id: string; comment: string; type: "u32" | "i32"; const: Const[] };
export type Fn = { name: string; returns: string; params: [string, string][]; doc?: string; import?: { module: string; name: string } };
export type Spec = { version: number; name: string; title: string; intro: string[]; impl_attribute?: string; group: Group[]; function: Fn[] };

export async function loadSpec(dir = root): Promise<Spec> {
  return Bun.TOML.parse(await Bun.file(join(dir, "abi/ay.abi.toml")).text()) as unknown as Spec;
}

const SCALARS = ["u32", "i32", "u64", "usize", "void"];
const POINTER = /^\*(const|mut) (u32|i32|u64|usize)$/;

export function validate(spec: Spec): string[] {
  const problems: string[] = [];
  const names = new Set<string>();
  const values = new Map<string, string>();
  for (const g of spec.group) {
    for (const c of g.const) {
      if (names.has(c.name)) problems.push(`duplicate name ${c.name}`);
      names.add(c.name);
      const range = g.type === "u32" ? [0, 0xffffffff] : [-0x80000000, 0x7fffffff];
      if (!Number.isInteger(c.value) || c.value < range[0]! || c.value > range[1]!) problems.push(`${c.name}: ${c.value} does not fit ${g.type}`);
      const key = `${g.id}:${c.value}`;
      if (values.has(key)) problems.push(`${c.name} and ${values.get(key)} share the value ${c.value} in group ${g.id}`);
      values.set(key, c.name);
      if (g.id === "syscall" && c.nargs === undefined) problems.push(`${c.name}: a syscall needs nargs`);
    }
  }
  for (const f of spec.function) {
    if (names.has(f.name)) problems.push(`duplicate name ${f.name}`);
    names.add(f.name);
    for (const t of [f.returns, ...f.params.map((p) => p[1])]) if (!SCALARS.includes(t) && !POINTER.test(t)) problems.push(`${f.name}: unknown type ${t}`);
  }
  return problems;
}

// ------------------------------------------------------------------------------------------ emitters

const C_NAME: Record<string, string> = { u32: "uint32_t", i32: "int32_t", u64: "uint64_t", usize: "uintptr_t", void: "void" };
const cTy = (t: string): string => {
  const m = POINTER.exec(t);
  return m ? `${m[1] === "const" ? "const " : ""}${C_NAME[m[2]!]} *` : C_NAME[t]!;
};
const cDecl = (f: Fn): string => {
  const ret = cTy(f.returns).padEnd(f.returns === "void" ? 0 : 9);
  const params = f.params.length ? f.params.map(([n, t]) => `${cTy(t)}${t.startsWith("*") ? "" : " "}${n}`).join(", ") : "void";
  return `${ret}${f.name}(${params});`;
};
const lit = (c: Const, g: Group) => (c.value < 0 ? `(${c.value})` : g.type === "u32" && g.id === "right" ? `${c.value}u` : String(c.value));

export function emitC(s: Spec): string {
  const guard = `${s.name.toUpperCase()}_ABI_H`;
  const out = [`/*\n * ${s.title}\n *\n${s.intro.map((l) => ` * ${l}`).join("\n")}\n */`, `#ifndef ${guard}`, `#define ${guard}`, "", "#include <stdint.h>", ""];
  for (const g of s.group) {
    if (g.id === "misc") out.push(`/* ${g.comment} */`);
    else out.push(`/* ${g.comment} */`);
    const width = Math.max(...g.const.map((c) => c.name.length)) + 1;
    const lits = g.const.map((c) => lit(c, g));
    const lw = Math.max(...lits.map((l) => l.length));
    g.const.forEach((c, i) => out.push(`#define ${c.name.padEnd(width)} ${c.doc ? lits[i]!.padEnd(lw) + `  /* ${c.doc} */` : lits[i]}`.trimEnd()));
    out.push("");
  }
  const entry = s.function.find((f) => f.import);
  if (entry) {
    out.push(`/* ${entry.doc ?? ""} On WebAssembly it is the import ("${entry.import!.module}", "${entry.import!.name}"); elsewhere the platform provides it.`);
    out.push(` * The corelib itself defines it, so its own sources set ${s.name.toUpperCase()}_CORELIB_IMPL and skip this declaration. */`);
    out.push(`#ifndef ${s.name.toUpperCase()}_CORELIB_IMPL`, "#ifdef __wasm__", `__attribute__((import_module("${entry.import!.module}"), import_name("${entry.import!.name}")))`, "#endif", cDecl(entry), "#endif", "");
  }
  out.push("/* Pure functions the corelib exports for hosts and tools (no capability needed). */");
  for (const f of s.function) if (!f.import) out.push(cDecl(f));
  out.push("", `#endif /* ${guard} */`, "");
  return out.join("\n");
}

const RUST_NAME = (t: string) => t;
const rustSig = (f: Fn) => `pub fn ${f.name}(${f.params.map(([n, t]) => `${n}: ${RUST_NAME(t)}`).join(", ")})${f.returns === "void" ? "" : ` -> ${f.returns}`};`;

export function emitRust(s: Spec): string {
  const out = [`//! ${s.title} Generated from abi/${s.name}.abi.toml by tools/abi.ts; do not edit.`, ""];
  for (const g of s.group) {
    out.push(`// ${g.comment}`);
    for (const c of g.const) out.push(`pub const ${c.name}: ${g.type} = ${c.value};`);
    out.push("");
  }
  const entry = s.function.find((f) => f.import);
  if (entry) {
    out.push(`#[cfg_attr(target_arch = "wasm32", link(wasm_import_module = "${entry.import!.module}"))]`, 'unsafe extern "C" {');
    out.push(`    #[cfg_attr(target_arch = "wasm32", link_name = "${entry.import!.name}")]`, `    ${rustSig(entry)}`, "}", "");
  }
  out.push('unsafe extern "C" {');
  for (const f of s.function) if (!f.import) out.push(`    ${rustSig(f)}`);
  out.push("}", "");
  return out.join("\n");
}

export function emitTs(s: Spec): string {
  const out = [`// ${s.title} Generated from abi/${s.name}.abi.toml by tools/abi.ts; do not edit.`, 'import { dlopen } from "bun:ffi";', ""];
  for (const g of s.group) {
    out.push(`// ${g.comment}`);
    for (const c of g.const) out.push(`export const ${c.name} = ${c.value};`);
    out.push("");
  }
  const calls = s.group.find((g) => g.id === "syscall");
  if (calls) {
    out.push("/** Number of arguments each syscall reads. */", "export const SYSCALL_NARGS: Readonly<Record<number, number>> = {");
    for (const c of calls.const) out.push(`  [${c.name}]: ${c.nargs},`);
    out.push("};", "");
  }
  const ffi = (t: string) => (t.startsWith("*") ? "ptr" : t === "void" ? "void" : t);
  out.push("/** Symbols of the corelib as `bun:ffi` describes them (the syscall entry is provided by the platform). */", "export const symbols = {");
  for (const f of s.function) if (!f.import) out.push(`  ${f.name}: { args: [${f.params.map(([, t]) => `"${ffi(t)}"`).join(", ")}], returns: "${ffi(f.returns)}" },`);
  out.push("} as const;", "", "/** Opens a built corelib (shared library) and binds every symbol. */", "export const open = (path: string) => dlopen(path, symbols);", "");
  return out.join("\n");
}

const PY: Record<string, string> = { u32: "ctypes.c_uint32", i32: "ctypes.c_int32", u64: "ctypes.c_uint64", usize: "ctypes.c_size_t", void: "None" };
const pyTy = (t: string) => {
  const m = POINTER.exec(t);
  return m ? `ctypes.POINTER(${PY[m[2]!]})` : PY[t]!;
};

export function emitPython(s: Spec): string {
  const out = [`"""${s.title} Generated from abi/${s.name}.abi.toml by tools/abi.ts; do not edit."""`, "import ctypes", ""];
  for (const g of s.group) {
    out.push(`# ${g.comment}`);
    for (const c of g.const) out.push(`${c.name} = ${c.value}`);
    out.push("");
  }
  out.push("SIGNATURES = {");
  for (const f of s.function) if (!f.import) out.push(`    "${f.name}": (${pyTy(f.returns)}, [${f.params.map(([, t]) => pyTy(t)).join(", ")}]),`);
  out.push("}", "", "", "def bind(lib):", '    """Sets restype and argtypes of every corelib function on a loaded library."""', "    for name, (restype, argtypes) in SIGNATURES.items():", "        fn = getattr(lib, name)", "        fn.restype = restype", "        fn.argtypes = argtypes", "    return lib", "");
  return out.join("\n");
}

export function emitJson(s: Spec, specText: string): string {
  const hash = new Bun.CryptoHasher("sha256").update(specText).digest("hex");
  return JSON.stringify({ version: s.version, name: s.name, spec_sha256: hash, constants: s.group.flatMap((g) => g.const.map((c) => ({ group: g.id, type: g.type, ...c }))), functions: s.function }, null, 2) + "\n";
}

export function emitDoc(s: Spec): string {
  const out = [`# ${s.name.toUpperCase()} ABI, version ${s.version}`, "", `<!-- Generated from abi/${s.name}.abi.toml by tools/abi.ts; do not edit. -->`, "", s.intro.join(" "), ""];
  for (const g of s.group) {
    out.push(`## ${g.comment.replace(/\.$/, "")}`, "", g.id === "syscall" ? "| Name | Value | Args | Meaning |\n|---|---:|---:|---|" : "| Name | Value | Meaning |\n|---|---:|---|");
    for (const c of g.const) out.push(g.id === "syscall" ? `| \`${c.name}\` | ${c.value} | ${c.nargs} | ${c.doc ?? ""} |` : `| \`${c.name}\` | ${c.value} | ${c.doc ?? ""} |`);
    out.push("");
  }
  out.push("## Functions", "", "| Function | Signature | Meaning |", "|---|---|---|");
  for (const f of s.function) out.push(`| \`${f.name}\` | \`${cDecl(f).replace(/\s+/g, " ")}\` | ${f.doc ?? ""}${f.import ? ` Import \`${f.import.module}.${f.import.name}\` on WebAssembly.` : ""} |`);
  out.push("");
  return out.join("\n");
}

export type Output = { path: string; text: string };

export function outputs(s: Spec, specText: string): Output[] {
  const n = s.name;
  return [
    { path: `corelib/include/${n}_abi.h`, text: emitC(s) },
    { path: `abi/gen/${n}_abi.rs`, text: emitRust(s) },
    { path: `abi/gen/${n}_abi.ts`, text: emitTs(s) },
    { path: `abi/gen/${n}_abi.py`, text: emitPython(s) },
    { path: `abi/gen/${n}_abi.json`, text: emitJson(s, specText) },
    { path: `docs/abi.md`, text: emitDoc(s) },
  ];
}

// ------------------------------------------------------------------------------------------ checks

const same = (a: Sig, b: Sig) => a.ret === b.ret && a.params.length === b.params.length && a.params.every((p, i) => p === b.params[i]);
const show = (s: Sig) => `${s.ret}(${s.params.join(", ")})`;

export async function check(s: Spec, files: Output[], dir = root): Promise<string[]> {
  const problems: string[] = [];
  const expectConsts = new Map(s.group.flatMap((g) => g.const.map((c) => [c.name, c.value] as const)));
  const expectFns = new Map(s.function.map((f) => [f.name, { ret: f.returns, params: f.params.map((p) => p[1]) }] as const));
  const compareConsts = (label: string, got: Map<string, number>) => {
    for (const [k, v] of expectConsts) if (got.get(k) !== v) problems.push(`${label}: ${k} is ${got.get(k) ?? "missing"}, the spec says ${v}`);
  };
  const compareFns = (label: string, got: Map<string, Sig>, only?: (f: Fn) => boolean) => {
    for (const f of s.function) {
      if (only && !only(f)) continue;
      const g = got.get(f.name);
      if (!g) problems.push(`${label}: ${f.name} is missing`);
      else if (!same(g, expectFns.get(f.name)!)) problems.push(`${label}: ${f.name} is ${show(g)}, the spec says ${show(expectFns.get(f.name)!)}`);
    }
  };

  const header = files.find((f) => f.path.endsWith(".h"))!;
  // tree-sitter cannot parse a preprocessor conditional that splits a declaration: drop the wasm import attribute block.
  const hRoot = await parse("c", header.text.replace(/#ifdef __wasm__\n[^\n]*\n#endif\n/g, ""));
  for (const e of syntaxErrors(hRoot)) problems.push(`${header.path}: syntax error at ${e}`);
  const h = cFacts(hRoot);
  compareConsts(header.path, h.consts);
  compareFns(header.path, h.fns);

  const rs = files.find((f) => f.path.endsWith(".rs"))!;
  // The published tree-sitter-rust does not know `unsafe extern` blocks yet: parse them as plain `extern` blocks.
  const rRoot = await parse("rust", rs.text.replaceAll('unsafe extern "C"', 'extern "C"'));
  for (const e of syntaxErrors(rRoot)) problems.push(`${rs.path}: syntax error at ${e}`);
  const r = rustFacts(rRoot);
  compareConsts(rs.path, r.consts);
  compareFns(rs.path, r.fns);

  const py = files.find((f) => f.path.endsWith(".py"))!;
  const pRoot = await parse("python", py.text);
  for (const e of syntaxErrors(pRoot)) problems.push(`${py.path}: syntax error at ${e}`);
  const p = pythonFacts(pRoot);
  compareConsts(py.path, p.consts);
  for (const f of s.function.filter((f) => !f.import)) if (p.arity.get(f.name) !== f.params.length) problems.push(`${py.path}: ${f.name} has ${p.arity.get(f.name) ?? "no"} argtypes, the spec says ${f.params.length}`);

  // The corelib: exactly the spec's functions, same signatures, same syscall arities.
  const srcDir = join(dir, "corelib/src");
  if (existsSync(srcDir)) {
    const attr = s.impl_attribute ?? "AY_API";
    const defined = new Map<string, Sig>();
    const exported = new Set<string>();
    let arity: Map<number, number> | undefined;
    for (const file of readdirSync(srcDir).filter((f) => f.endsWith(".c")).sort()) {
      const text = await Bun.file(join(srcDir, file)).text();
      for (const m of text.matchAll(new RegExp(`${attr}\\s+[^;{]*?\\b(${s.name}_\\w+)\\s*\\(`, "g"))) exported.add(m[1]!);
      const tree = await parse("c", text.replaceAll(attr, " ".repeat(attr.length)));
      for (const d of tree.descendantsOfType("function_definition")) {
        const name = d?.childForFieldName("declarator")?.descendantsOfType("identifier")[0]?.text;
        if (!name?.startsWith(`${s.name}_`)) continue;
        const facts = cFacts(d!);
        const sig = facts.fns.get(name);
        if (sig) defined.set(name, sig);
      }
      arity ??= switchTable(tree, `${s.name}_arg_count`, expectConsts);
    }
    compareFns("corelib/src", defined, (f) => !f.import);
    for (const name of exported) if (!expectFns.has(name)) problems.push(`corelib/src: ${name} is exported (${attr}) but not in the spec`);
    const calls = s.group.find((g) => g.id === "syscall");
    if (calls && arity) {
      for (const c of calls.const) if (arity.get(c.value) !== c.nargs) problems.push(`corelib/src: ${s.name}_arg_count(${c.name}) returns ${arity.get(c.value) ?? "nothing"}, the spec says ${c.nargs}`);
      if (arity.get(-1) !== 0) problems.push(`corelib/src: ${s.name}_arg_count has no default returning 0`);
    }
  }
  return problems;
}

if (import.meta.main) {
  const checkOnly = process.argv.includes("--check");
  const specText = await Bun.file(join(root, "abi/ay.abi.toml")).text();
  const spec = await loadSpec();
  const problems = validate(spec);
  const files = outputs(spec, specText);
  for (const f of files) {
    const path = join(root, f.path);
    const current = (await Bun.file(path).exists()) ? await Bun.file(path).text() : undefined;
    if (current === f.text) continue;
    if (checkOnly) problems.push(`${f.path} is stale: run bun tools/abi.ts`);
    else await Bun.write(path, f.text);
  }
  if (!problems.length) problems.push(...(await check(spec, files)));
  console.log(`${spec.group.reduce((n, g) => n + g.const.length, 0)} constants, ${spec.function.length} functions, ${files.length} outputs`);
  for (const p of problems) console.error(p);
  process.exit(problems.length ? 1 : 0);
}
