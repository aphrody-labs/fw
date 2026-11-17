#!/usr/bin/env bun
// Regenerates the code listings of docs/guides/assembly-and-wasm* from the code base, so the guide cannot drift.
//   bun tools/listings.ts          rewrite every listing block in place
//   bun tools/listings.ts --check  fail when a block differs from what the code generates now
//   either form accepts file paths (relative to the repository) to handle only those guide files
//
// A block is `<!-- listing SPEC -->` ... `<!-- /listing -->`; SPEC is one of
//   c:PATH#FN                 a C function with its comment
//   asm:FN:ARCH               the function compiled by `clang -S` for ARCH (x86_64 | aarch64 | riscv64), flags of the images
//   wat:FN[:VARIANT]          the function in the linked corelib.wasm, printed as WAT; VARIANT is base | simd | atomics
//   file:PATH[#FIRST-LAST]    a repository file, optionally a line range
//   dump:PATH:LINES           the first LINES of `wasm-tools dump` of a .wat file, assembled
// The clang major version and the wasm-tools version are pinned in docs/guides/assembly-and-wasm/listings.toml.
import { existsSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { extname, join } from "node:path";

import { ASM_TARGETS, buildWasm, clangVersion, emitAsm, root, run, type WasmVariant } from "./corelib.ts";

const GUIDE = "docs/guides/assembly-and-wasm";
const PINS = `${GUIDE}/listings.toml`;

export type Pins = { clang: string; wasm_tools: string };

const LISTING_VARIANTS = new Set<string>(["base", "simd", "atomics"]);

// ---------------------------------------------------------------- pure text extraction

/** The body of function `name` in clang's `-S` output, cleaned of assembler bookkeeping. */
export function extractAsm(asm: string, name: string): string {
  const lines = asm.split("\n");
  const start = lines.findIndex(l => l.startsWith(`${name}:`));
  if (start < 0) throw new Error(`asm: function ${name} not found`);
  let end = start + 1;
  while (end < lines.length && !/^\.Lfunc_end\d+:/.test(lines[end]!)) end++;
  const body = [`${name}:`, ...lines.slice(start + 1, end)];
  const refs = new Set(body.join("\n").match(/\.L(?:JTI|CPI|switch\.table)[\w.$]*/g) ?? []);
  const data: string[] = [];
  for (const ref of refs) {
    const at = lines.findIndex(l => l.startsWith(`${ref}:`));
    if (at < 0) continue;
    data.push(`${ref}:`);
    for (let i = at + 1; i < lines.length && /^\t\.(?:long|word|quad|byte|short|zero|ascii|asciz|xword|dword|hword)\b/.test(lines[i]!); i++) data.push(lines[i]!);
  }
  return [...body, ...data].filter(keepAsmLine).map(stripComment).join("\n");
}

function keepAsmLine(l: string): boolean {
  if (l.trim() === "") return false;
  if (/^\s*(?:#|\/\/|;)/.test(l)) return false; // comment-only lines such as `# %bb.0:`
  if (/^\t\.(?:cfi_|p2align|balign|size|type|globl|global|hidden|section|file|loc|ident|attribute|option|text|addrsig|functype)/.test(l)) return false;
  if (/^\.(?:Ltmp|Lfunc_begin|Lfunc_end)\d*:/.test(l)) return false;
  return true;
}

function stripComment(l: string): string {
  // `# note` (x86, RISC-V) and `// note` (AArch64) are comments; AArch64 immediates such as `#40389` are not.
  return l.replace(/\s+(?:#(?=[\s=])|;|\/\/).*$/, "");
}

/** The `(func ...)` block of `wasm-tools print` output for an exported function, without volatile indices. */
export function extractWat(wat: string, name: string): string {
  const lines = wat.split("\n");
  let start = lines.findIndex(l => l.startsWith(`  (func $${name} `) || l === `  (func $${name}`);
  if (start >= 0) {
    // Some wasm-ld builds write a name section: spell the header the same way as the unnamed case.
    if (!lines[start]!.includes(`(export "${name}")`)) lines[start] = lines[start]!.replace(`(func $${name}`, `(func $${name} (export "${name}")`);
  } else {
    // No name section (the usual wasm-ld output): find the function through its export and give it the name.
    const index = new RegExp(`\\(export "${name}" \\(func (\\d+)\\)\\)`).exec(wat)?.[1];
    if (index === undefined) throw new Error(`wat: function ${name} is not exported`);
    start = lines.findIndex(l => l.startsWith(`  (func (;${index};)`));
    if (start < 0) throw new Error(`wat: function ${name} (index ${index}) not found`);
    lines[start] = lines[start]!.replace(`(func (;${index};)`, `(func $${name} (export "${name}")`);
  }
  let end = start + 1;
  while (end < lines.length && !/^ {2}\(/.test(lines[end]!)) end++;
  return lines
    .slice(start, end)
    .map(l => l.replace(/ \(;\d+;\)/g, "").replace(/^ {2}/, ""))
    .join("\n")
    .trimEnd();
}

/** Function `name` from C source: the comment lines above it, the signature and the body. */
export function extractCFn(src: string, name: string): string {
  const lines = src.split("\n");
  const def = new RegExp(`^[A-Za-z_\\[].*\\b${name}\\(.*[^;\\s]\\s*$`);
  const at = lines.findIndex(l => def.test(l) && !l.startsWith("//") && !/^\s*(?:return|if|for|while)\b/.test(l));
  if (at < 0) throw new Error(`c: function ${name} not found`);
  let first = at;
  while (first > 0 && /^\s*\/\//.test(lines[first - 1]!)) first--;
  let depth = 0;
  let seen = false;
  for (let i = at; i < lines.length; i++) {
    for (const ch of lines[i]!) {
      if (ch === "{") {
        depth++;
        seen = true;
      } else if (ch === "}") depth--;
    }
    if (seen && depth === 0) return lines.slice(first, i + 1).join("\n");
  }
  throw new Error(`c: unterminated function ${name}`);
}

const FENCE: Record<string, string> = { ".wat": "wat", ".c": "c", ".h": "c", ".s": "asm", ".ts": "ts", ".toml": "toml", ".ld": "text", ".txt": "text", ".md": "markdown", ".wit": "text" };

export type Replacer = (spec: string) => { lang: string; body: string };

/** Rewrite every listing block of `md` using `generate`. */
export function rewrite(md: string, generate: Replacer): string {
  return md.replace(/(<!-- listing (\S+) -->\n)[\s\S]*?(<!-- \/listing -->)/g, (_m, open: string, spec: string, close: string) => {
    const { lang, body } = generate(spec);
    return `${open}\`\`\`${lang}\n${body}\n\`\`\`\n${close}`;
  });
}

// ---------------------------------------------------------------- generation

export class Generator {
  private asmCache = new Map<string, string>();
  private watCache = new Map<string, string>();
  private tmp = mkdtempSync(join(tmpdir(), "ay-listings-"));

  constructor(private pins: Pins) {}

  /** Empty when the installed tools are the pinned ones; AY_LISTINGS_SKIP_PIN=1 skips the check to diagnose another toolchain. */
  verifyTools(): string[] {
    const out: string[] = [];
    if (process.env.AY_LISTINGS_SKIP_PIN) return out;
    const clang = clangVersion();
    if (!clang.startsWith(`${this.pins.clang}.`)) out.push(`clang ${this.pins.clang}.x pinned, found ${clang}`);
    const wt = run(["wasm-tools", "--version"]);
    if (!wt.startsWith(`wasm-tools ${this.pins.wasm_tools} `)) out.push(`wasm-tools ${this.pins.wasm_tools} pinned, found ${wt.trim()}`);
    return out;
  }

  private asm(arch: string): string {
    if (!ASM_TARGETS[arch]) throw new Error(`asm: unknown arch ${arch}`);
    const cached = this.asmCache.get(arch);
    if (cached) return cached;
    const text = emitAsm(arch);
    this.asmCache.set(arch, text);
    return text;
  }

  private wat(variant: string): string {
    if (!LISTING_VARIANTS.has(variant)) throw new Error(`wat: unknown variant ${variant}`);
    const cached = this.watCache.get(variant);
    if (cached) return cached;
    const text = run(["wasm-tools", "print", buildWasm(variant as WasmVariant)]);
    this.watCache.set(variant, text);
    return text;
  }

  generate: Replacer = spec => {
    const [kind, ...rest] = spec.split(":");
    switch (kind) {
      case "c": {
        const [path, fn] = rest.join(":").split("#");
        return { lang: "c", body: extractCFn(readFileSync(join(root, path!), "utf8"), fn!) };
      }
      case "asm": {
        const [fn, arch] = rest;
        return { lang: "asm", body: extractAsm(this.asm(arch!), fn!) };
      }
      case "wat": {
        const [fn, variant = "base"] = rest;
        return { lang: "wat", body: extractWat(this.wat(variant), fn!) };
      }
      case "file": {
        const [path, range] = rest.join(":").split("#");
        let text = readFileSync(join(root, path!), "utf8").trimEnd();
        if (range) {
          const [a, b] = range.split("-").map(Number);
          text = text.split("\n").slice(a! - 1, b).join("\n");
        }
        return { lang: FENCE[extname(path!)] ?? "text", body: text };
      }
      case "dump": {
        const [path, lines] = [rest.slice(0, -1).join(":"), Number(rest.at(-1))];
        const wasm = join(this.tmp, "dump.wasm");
        run(["wasm-tools", "parse", join(root, path), "-o", wasm]);
        return { lang: "text", body: run(["wasm-tools", "dump", wasm]).split("\n").slice(0, lines).join("\n").trimEnd() };
      }
      default:
        throw new Error(`unknown listing kind in ${spec}`);
    }
  };

  close(): void {
    rmSync(this.tmp, { recursive: true, force: true });
  }
}

// ---------------------------------------------------------------- driver

export function guideFiles(): string[] {
  const dir = join(root, GUIDE);
  const chapters = existsSync(dir) ? readdirSync(dir).filter(f => f.endsWith(".md")).sort().map(f => `${GUIDE}/${f}`) : [];
  return [`${GUIDE}.md`, ...chapters].filter(f => existsSync(join(root, f)));
}

if (import.meta.main) {
  const check = process.argv.includes("--check");
  const pins = Bun.TOML.parse(readFileSync(join(root, PINS), "utf8")) as Pins;
  const gen = new Generator(pins);
  const problems: string[] = [];
  try {
    problems.push(...gen.verifyTools());
    if (problems.length === 0) {
      const only = process.argv.slice(2).filter(a => !a.startsWith("--"));
      for (const file of only.length ? only : guideFiles()) {
        const before = readFileSync(join(root, file), "utf8");
        const after = rewrite(before, gen.generate);
        if (after === before) continue;
        if (check) {
          const a = before.split("\n");
          const b = after.split("\n");
          const at = a.findIndex((l, i) => l !== b[i]);
          problems.push(`${file}: listings are stale, first difference at line ${at + 1}:\n  committed: ${a[at]}\n  generated: ${b[at]}`);
        } else writeFileSync(join(root, file), after);
      }
    }
  } finally {
    gen.close();
  }
  for (const p of problems) console.error(`listings: ${p}`);
  if (problems.length) {
    console.error("listings: run `bun tools/listings.ts` with the pinned toolchain and commit the result");
    process.exit(1);
  }
  console.log(check ? "listings: up to date" : "listings: written");
}
