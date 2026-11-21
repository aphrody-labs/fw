#!/usr/bin/env bun
// Generates the Linux UAPI TypeScript package from `make headers_install` output.
//
//   bun gen/gen.ts --headers <dir> --arch x86_64|aarch64 --kernel-version v7.3.5 --out src/x86_64
//   bun gen/gen.ts --headers <root> [--kernel-version v7.3.5] [--out src]     every arch, <root>/<arch>/ each
//   bun gen/gen.ts --check --headers <root>                                   regenerate to a temp dir and diff
//
// Needs clang, pahole and bpftool. Pipeline per arch: all-headers translation unit, DWARF -> BTF
// (pahole -J, bpftool), compiler-evaluated macros, alignment/size/offset cross-check, ioctl and
// syscall tables, emit. Output is deterministic (no timestamps, no absolute paths).
import { mkdtemp, mkdir, readdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { parseArgs } from "node:util";
import { ARCHES, archOf, type Arch } from "./lib/arch.ts";
import { readModel } from "./lib/btf.ts";
import { checkIdentifier, evaluateMacros, mergeConstants } from "./lib/constants.ts";
import { emitConstants, emitEnums, emitIndex, emitIoctls, emitMeta, emitStructs, emitSyscalls, type Provenance } from "./lib/emit.ts";
import { Timings } from "./lib/exec.ts";
import { collectIoctls } from "./lib/ioctls.ts";
import { verifyLayouts } from "./lib/layouts.ts";
import { readMacros } from "./lib/preprocess.ts";
import { collectSyscalls } from "./lib/syscalls.ts";
import { buildTu } from "./lib/tu.ts";

const packageDir = resolve(import.meta.dir, "..");

interface Options {
  headers: string;
  arch?: string;
  out: string;
  kernelVersion?: string;
  kernelCommit?: string;
  check: boolean;
}

function usage(message: string): never {
  console.error(`${message}\nusage: bun gen/gen.ts --headers <dir> [--arch x86_64|aarch64] [--kernel-version v7.3.5] [--out <dir>] [--check]`);
  process.exit(2);
}

async function pinnedCommit(tag: string): Promise<string> {
  const file = resolve(packageDir, "../../kernel.toml");
  try {
    const parsed = Bun.TOML.parse(await readFile(file, "utf8")) as { tree?: { tag?: string; commit?: string }[] };
    return parsed.tree?.find((tree) => tree.tag === tag)?.commit ?? "";
  } catch {
    return "";
  }
}

async function headersVersion(headers: string): Promise<string> {
  const text = await readFile(join(headers, "include/linux/version.h"), "utf8");
  const part = (name: string): string => new RegExp(`#define LINUX_VERSION_${name} (\\d+)`).exec(text)?.[1] ?? "?";
  return `v${part("MAJOR")}.${part("PATCHLEVEL")}.${part("SUBLEVEL")}`;
}

async function generateArch(arch: Arch, headers: string, provenance: Provenance, outDir: string, timings: Timings): Promise<Record<string, number>> {
  const work = await mkdtemp(join(tmpdir(), `linux-types-${arch.name}-`));
  try {
    const stage = <T>(name: string, body: () => Promise<T>) => timings.time(`${arch.name} ${name}`, body);
    const tu = await stage("translation unit", () => buildTu(headers, arch, work));
    const model = await stage("DWARF, BTF", () => readModel(tu));
    const verified = await stage("layout cross-check", () => verifyLayouts(model, tu));
    const macros = await stage("macro table", () => readMacros(tu.cflags, tu.sourcePath, tu.includeDir, tu.stubDir));
    const evaluation = await stage("macro evaluation", () => evaluateMacros(tu, macros));
    const ioctl = await stage("ioctls", () => collectIoctls(tu, macros, evaluation.evaluated, model));
    const syscalls = await stage("syscalls", () => collectSyscalls(tu));
    const { constants, shadowedMacros } = mergeConstants(model, macros, evaluation.evaluated, ioctl.names);

    // One namespace per arch module: every export name must be unique.
    const owners = new Map<string, string>();
    const claim = (name: string, owner: string): void => {
      const previous = owners.get(name);
      if (previous) throw new Error(`duplicate export ${name}: ${previous} and ${owner}`);
      owners.set(name, owner);
    };
    for (const name of ["ioctls", "IoctlName", "syscalls", "SyscallName", "syscallNames", "meta"]) claim(name, "fixed export");
    for (const constant of constants) claim(constant.name, constant.header ? `macro in ${constant.header}` : `enumerator of ${constant.enumName}`);
    for (const key of model.records.keys()) claim(key, "record");
    for (const key of model.enums.keys()) claim(key, "enum");
    for (const [alias, target] of model.aliases) {
      if (model.records.has(alias) || model.enums.has(alias)) continue;
      checkIdentifier(alias, "typedef alias");
      claim(alias, `typedef alias of ${target}`);
    }
    for (const entry of syscalls.syscalls) checkIdentifier(entry.name, "syscall");

    const counts: Record<string, number> = {
      structs: [...model.records.keys()].filter((key) => key.startsWith("struct_")).length,
      unions: [...model.records.keys()].filter((key) => key.startsWith("union_")).length,
      typedefRecords: [...model.records.keys()].filter((key) => key.startsWith("typedef_")).length,
      enums: model.enums.size,
      typedefAliases: [...model.aliases.keys()].filter((key) => !model.records.has(key) && !model.enums.has(key)).length,
      enumerators: constants.filter((constant) => constant.header === "").length,
      macroConstants: constants.filter((constant) => constant.header !== "").length,
      macrosNotConstant: evaluation.notConstants,
      macrosAmbiguous: macros.ambiguous.size,
      macrosShadowingEnumerators: shadowedMacros.length,
      ioctls: ioctl.ioctls.length,
      ioctlsWithStructArgument: ioctl.ioctls.filter((entry) => entry.recordKey !== null).length,
      ioctlSizeMismatches: ioctl.sizeMismatches,
      syscalls: syscalls.syscalls.length,
      sizesChecked: verified.checkedSizes,
      offsetsChecked: verified.checkedOffsets,
    };

    const files: Record<string, string> = {
      "structs.ts": emitStructs(model, provenance),
      "enums.ts": emitEnums(model, provenance),
      "constants.ts": emitConstants(constants, provenance),
      "ioctls.ts": emitIoctls(ioctl.ioctls, provenance),
      "syscalls.ts": emitSyscalls(syscalls, provenance),
      "meta.ts": emitMeta(provenance, counts, { included: tu.headers.length, excluded: tu.excluded }),
      "index.ts": emitIndex(provenance),
    };
    await mkdir(outDir, { recursive: true });
    for (const [name, text] of Object.entries(files)) await writeFile(join(outDir, name), text);
    return counts;
  } finally {
    await rm(work, { recursive: true, force: true });
  }
}

async function listFiles(dir: string): Promise<string[]> {
  const entries = await readdir(dir, { recursive: true, withFileTypes: true }).catch(() => []);
  return entries.filter((entry) => entry.isFile()).map((entry) => join(entry.parentPath, entry.name).slice(dir.length + 1).replaceAll("\\", "/")).sort();
}

async function main(): Promise<void> {
  const { values } = parseArgs({
    options: {
      headers: { type: "string" },
      arch: { type: "string" },
      out: { type: "string" },
      "kernel-version": { type: "string" },
      "kernel-commit": { type: "string" },
      check: { type: "boolean", default: false },
    },
    strict: true,
  });
  if (!values.headers) usage("--headers is required");
  const options: Options = {
    headers: resolve(values.headers),
    arch: values.arch,
    out: resolve(values.out ?? join(packageDir, "src", values.arch ?? "")),
    kernelVersion: values["kernel-version"],
    kernelCommit: values["kernel-commit"],
    check: values.check,
  };
  const targets = options.arch ? [archOf(options.arch)] : Object.values(ARCHES);
  const timings = new Timings();
  const totalStart = performance.now();
  const writeRoot = options.check ? await mkdtemp(join(tmpdir(), "linux-types-check-")) : undefined;
  let drift = 0;
  try {
    for (const arch of targets) {
      const headers = options.arch ? options.headers : join(options.headers, arch.name);
      const detected = await headersVersion(headers);
      const kernelTag = options.kernelVersion ? (options.kernelVersion.startsWith("v") ? options.kernelVersion : `v${options.kernelVersion}`) : detected;
      if (kernelTag !== detected) throw new Error(`--kernel-version ${kernelTag} but ${headers} are the headers of ${detected}`);
      const provenance: Provenance = { kernelTag, kernelCommit: options.kernelCommit ?? (await pinnedCommit(kernelTag)), arch: arch.name };
      const committedDir = options.arch ? options.out : join(options.out, arch.name);
      const outDir = writeRoot ? join(writeRoot, arch.name) : committedDir;
      const counts = await generateArch(arch, headers, provenance, outDir, timings);
      console.error(`${arch.name}: ${JSON.stringify(counts)}`);
      if (writeRoot) {
        const fresh = await listFiles(outDir);
        const committed = await listFiles(committedDir);
        for (const name of new Set([...fresh, ...committed])) {
          const a = fresh.includes(name) ? await readFile(join(outDir, name), "utf8") : null;
          const b = committed.includes(name) ? await readFile(join(committedDir, name), "utf8") : null;
          if (a !== b) {
            drift++;
            console.error(`drift: ${join(committedDir, name)} ${a === null ? "is not generated any more" : b === null ? "is missing" : "differs"}`);
          }
        }
      }
    }
  } finally {
    if (writeRoot) await rm(writeRoot, { recursive: true, force: true });
  }
  for (const { stage, seconds } of timings.stages) console.error(`  ${stage}: ${seconds.toFixed(1)} s`);
  console.error(`total: ${((performance.now() - totalStart) / 1000).toFixed(1)} s`);
  if (options.check) {
    if (drift > 0) process.exit(1);
    console.error("no drift");
  }
}

await main();
