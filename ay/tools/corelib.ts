// Build the SDK corelib (freestanding C23 + assembly) with clang and lld: WebAssembly modules, the bare-metal x86_64
// image and per-target assembly. One place knows the flags; tests, QEMU runs and the listing generator call it.
import { existsSync, mkdirSync, readdirSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";

export const root = resolve(import.meta.dir, "..");
const SRC = "corelib/src";
const COMMON = ["-std=c23", "-O2", "-ffreestanding", "-fno-stack-protector", "-fvisibility=hidden", "-Wall", "-Wextra", "-Werror", `-I${join(root, "corelib/include")}`, `-I${join(root, SRC)}`];

/** Sources every build shares; platform files (wasm.c, arch/*) are added per target. */
export function commonSources(): string[] {
  return readdirSync(join(root, SRC)).filter(f => f.endsWith(".c") && f !== "wasm.c").sort().map(f => `${SRC}/${f}`);
}

export type WasmVariant = "base" | "simd" | "atomics" | "component";
const WASM_FLAGS: Record<WasmVariant, string[]> = {
  base: [],
  simd: ["-msimd128"],
  atomics: ["-matomics", "-mbulk-memory"],
  component: ["-DAY_COMPONENT"],
};

/** Native targets whose assembly the guide shows: clang triple and the flags that matter for them. */
export const ASM_TARGETS: Record<string, string[]> = {
  "x86_64": ["--target=x86_64-unknown-none-elf", "-masm=intel", "-mgeneral-regs-only", "-mno-red-zone"],
  "aarch64": ["--target=aarch64-none-elf"],
  "riscv64": ["--target=riscv64-unknown-elf", "-march=rv64gc", "-mabi=lp64d"],
};

export function findClang(): string {
  const candidates = [process.env.AY_CLANG, ...[23, 22, 21].map(v => Bun.which(`clang-${v}`)), Bun.which("clang"), "C:/Program Files/LLVM/bin/clang.exe"];
  const found = candidates.find((c): c is string => !!c && existsSync(c));
  if (!found) throw new Error("clang not found: install LLVM or set AY_CLANG");
  return found;
}

export function run(cmd: string[], cwd = root): string {
  const p = Bun.spawnSync(cmd, { cwd, stdout: "pipe", stderr: "pipe" });
  if (p.exitCode !== 0) throw new Error(`${cmd.join(" ")} failed (${p.exitCode}):\n${p.stderr.toString()}${p.stdout.toString()}`);
  return p.stdout.toString().replaceAll("\r\n", "\n");
}

export function clangVersion(): string {
  return /clang version (\S+)/.exec(run([findClang(), "--version"]))?.[1] ?? "unknown";
}

function outDir(name: string): string {
  const dir = join(root, "target/corelib", name);
  mkdirSync(dir, { recursive: true });
  return dir;
}

/** Build a WebAssembly module of the corelib; returns the .wasm path. */
export function buildWasm(variant: WasmVariant = "base"): string {
  const out = join(outDir(`wasm-${variant}`), "corelib.wasm");
  // --no-wasm-opt: clang otherwise runs binaryen's wasm-opt after linking when it is on PATH, and the output would depend on the machine.
  const link = ["--no-wasm-opt", "-Wl,--no-entry", "-Wl,-z,stack-size=65536", "-Wl,--initial-memory=131072", ...(variant === "component" ? [] : ["-Wl,--export-dynamic"])];
  run([findClang(), "--target=wasm32", "-mcpu=mvp", ...COMMON, ...WASM_FLAGS[variant], "-nostdlib", ...link, "-o", out, ...commonSources(), `${SRC}/wasm.c`]);
  return out;
}

/** Build the bare-metal x86_64 image (PVH boot); returns the ELF path. Needs lld. */
export function buildX86Image(): string {
  const out = join(outDir("x86_64"), "corelib.elf");
  const flags = ["--target=x86_64-unknown-none-elf", "-mgeneral-regs-only", "-mno-red-zone", "-fno-pic", "-fno-pie", "-mcmodel=small"];
  run([
    findClang(), ...flags, ...COMMON, "-nostdlib", "-static", "-fuse-ld=lld", "-Wl,--build-id=none", "-Wl,-z,noexecstack", "-Wl,-z,max-page-size=4096",
    `-Wl,-T,${join(root, "corelib/arch/x86_64/linker.ld")}`, "-o", out, ...commonSources(), "corelib/arch/x86_64/kmain.c", "corelib/arch/x86_64/boot.s",
  ]);
  return out;
}

/** Build the hosted test program with the platform C library (its own memcpy replaces libc.c); returns the executable path. */
export function buildHost(driver: "clang" | "clang-cl" = "clang"): string {
  const sources = commonSources().filter(f => !f.endsWith("/libc.c"));
  const exe = process.platform === "win32" ? ".exe" : "";
  const out = join(outDir(`host-${driver}`), `corelib${exe}`);
  const tests = "corelib/tests/host.c";
  if (driver === "clang-cl") {
    const cl = join(dirname(findClang()), process.platform === "win32" ? "clang-cl.exe" : "clang-cl");
    const objDir = `${outDir("host-clang-cl")}${process.platform === "win32" ? "\\" : "/"}`;
    run([cl, "/nologo", "/std:clatest", "/O2", "/W3", "/WX", "-fuse-ld=lld", `/I${join(root, "corelib/include")}`, `/I${join(root, SRC)}`, `/Fe:${out}`, `/Fo:${objDir}`, ...sources, tests]);
  } else {
    run([findClang(), "-std=c23", "-O2", "-Wall", "-Wextra", "-Werror", "-fuse-ld=lld", `-I${join(root, "corelib/include")}`, `-I${join(root, SRC)}`, "-o", out, ...sources, tests]);
  }
  return out;
}

/** Compile every corelib source to assembly for `arch` (a key of ASM_TARGETS); returns the concatenated text. */
export function emitAsm(arch: string): string {
  const flags = ASM_TARGETS[arch];
  if (!flags) throw new Error(`unknown asm target ${arch}`);
  const dir = outDir(`asm-${arch}`);
  return commonSources()
    .map(src => {
      const out = join(dir, `${src.split("/").pop()!.replace(/\.c$/, "")}.s`);
      run([findClang(), ...flags, ...COMMON, "-S", "-o", out, src]);
      return readFileSync(out, "utf8").replaceAll("\r\n", "\n");
    })
    .join("\n");
}

export function ensureDir(path: string): void {
  mkdirSync(dirname(path), { recursive: true });
}
