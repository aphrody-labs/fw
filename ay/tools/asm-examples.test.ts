// Assembles and runs the hand-written examples of docs/guides/assembly-and-wasm/ (corelib/asm/*.s).
// LLVM first (clang as assembler, ld.lld as linker), GNU binutils as the fallback. A test runs only when the host can
// really do it; otherwise it is skipped and the title says why.
//   bun test tools/asm-examples.test.ts
import { afterAll, describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const asmDir = resolve(import.meta.dir, "../corelib/asm");
const work = mkdtempSync(join(tmpdir(), "ay-asm-examples-"));
afterAll(() => rmSync(work, { recursive: true, force: true }));

type Arch = { arch: string; nodeArch: string; prefix: string; triple: string; bootTriple: string; clangFlags: string[]; hello: string; boot: string; qemuUser: string; qemuSystem: string; ldFlags: string[]; machine: string[]; expectBoot: string[] };

const ARCHS: Arch[] = [
  { arch: "x86_64", nodeArch: "x64", prefix: "", triple: "x86_64-linux-gnu", bootTriple: "", clangFlags: [], hello: "hello from x86-64\n", boot: "", qemuUser: "", qemuSystem: "", ldFlags: [], machine: [], expectBoot: [] },
  {
    arch: "aarch64",
    nodeArch: "arm64",
    prefix: "aarch64-linux-gnu-",
    triple: "aarch64-linux-gnu",
    bootTriple: "aarch64-none-elf",
    clangFlags: [],
    hello: "hello from AArch64\n",
    boot: "boot_virt_aarch64.s",
    qemuUser: "qemu-aarch64",
    qemuSystem: "qemu-system-aarch64",
    ldFlags: ["-nostdlib", "-Ttext=0x40000000"],
    machine: ["-M", "virt", "-cpu", "cortex-a72", "-nographic", "-semihosting"],
    expectBoot: ["hello from AArch64 EL1", "caught svc (ESR_EL1.EC == 0x15)", "returned from the handler, exiting"],
  },
  {
    arch: "riscv64",
    nodeArch: "riscv64",
    prefix: "riscv64-linux-gnu-",
    triple: "riscv64-linux-gnu",
    bootTriple: "riscv64-unknown-elf",
    clangFlags: ["-march=rv64gc"],
    hello: "hello from RISC-V\n",
    boot: "boot_virt_riscv64.s",
    qemuUser: "qemu-riscv64",
    qemuSystem: "qemu-system-riscv64",
    ldFlags: ["-N", "--no-warn-rwx-segments", "-nostdlib", "-Ttext=0x80200000"],
    machine: ["-M", "virt", "-nographic", "-bios", "default"],
    expectBoot: ["hello from RISC-V S-mode via the 16550", "hello again via SBI console_putchar", "caught ebreak (scause == 3)", "returned from the handler, shutting down"],
  },
];

const native = (a: Arch) => process.platform === "linux" && process.arch === a.nodeArch;
const missing = (names: string[]) => names.filter(n => n && !Bun.which(n));

const llvm = () => missing(["clang", "ld.lld"]).length === 0;

/** Assembler and linker commands: LLVM when clang and ld.lld exist, otherwise GNU (native `as`/`ld` or the cross prefix). */
function tools(a: Arch, boot: boolean) {
  if (llvm()) {
    const target = `--target=${boot ? a.bootTriple : a.triple}`;
    return {
      as: (src: string, obj: string) => ["clang", target, ...a.clangFlags, "-c", src, "-o", obj],
      ld: (flags: string[], obj: string, exe: string) => ["ld.lld", ...flags.filter(f => f !== "--no-warn-rwx-segments"), obj, "-o", exe],
      needs: ["clang", "ld.lld"],
    };
  }
  const as = native(a) ? "as" : `${a.prefix}as`;
  const ld = native(a) ? "ld" : `${a.prefix}ld`;
  return { as: (src: string, obj: string) => [as, src, "-o", obj], ld: (flags: string[], obj: string, exe: string) => [ld, ...flags, obj, "-o", exe], needs: [as, ld] };
}

/** Why the user-space example cannot run here, or undefined when it can. */
function helloSkip(a: Arch): string | undefined {
  if (process.platform !== "linux") return `needs a Linux host, this is ${process.platform}`;
  const lacking = missing([...tools(a, false).needs, ...(native(a) ? [] : [a.qemuUser])]);
  return lacking.length ? `missing ${lacking.join(", ")}` : undefined;
}

/** Why the bare-metal example cannot run here, or undefined when it can. */
function bootSkip(a: Arch): string | undefined {
  if (!a.boot) return "no bare-metal example for this architecture";
  if (process.platform !== "linux") return `needs a Linux host, this is ${process.platform}`;
  const lacking = missing([...tools(a, true).needs, a.qemuSystem]);
  return lacking.length ? `missing ${lacking.join(", ")}` : undefined;
}

function run(cmd: string[], timeout = 60_000) {
  const p = Bun.spawnSync(cmd, { stdin: "ignore", stdout: "pipe", stderr: "pipe", timeout });
  return { code: p.exitCode, out: p.stdout.toString().replace(/\r\n/g, "\n"), err: p.stderr.toString() };
}

function build(a: Arch, source: string, ldFlags: string[], boot: boolean): string {
  const chain = tools(a, boot);
  const obj = join(work, `${source}.o`);
  const exe = join(work, `${source}.elf`);
  const asm = run(chain.as(join(asmDir, source), obj));
  expect(asm.err).toBe("");
  expect(asm.code).toBe(0);
  const link = run(chain.ld(ldFlags, obj, exe));
  expect(link.err).toBe("");
  expect(link.code).toBe(0);
  return exe;
}

describe("Linux user-space hello, raw syscalls", () => {
  for (const a of ARCHS) {
    const skip = helloSkip(a);
    if (skip) console.log(`asm-examples: skip hello_linux_${a.arch}: ${skip}`);
    test.skipIf(!!skip)(`hello_linux_${a.arch}${skip ? ` (skipped: ${skip})` : ""}`, () => {
      const exe = build(a, `hello_linux_${a.arch}.s`, [], false);
      const r = run(native(a) ? [exe] : [a.qemuUser, exe]);
      expect(r.out).toBe(a.hello);
      expect(r.code).toBe(0);
    });
  }
});

describe("bare-metal on QEMU virt", () => {
  for (const a of ARCHS) {
    const skip = bootSkip(a);
    if (skip && a.boot) console.log(`asm-examples: skip ${a.boot}: ${skip}`);
    if (!a.boot) continue;
    test.skipIf(!!skip)(`${a.boot}${skip ? ` (skipped: ${skip})` : ""}`, () => {
      const exe = build(a, a.boot, a.ldFlags, true);
      const r = run([a.qemuSystem, ...a.machine, "-kernel", exe], 30_000);
      expect(r.code).toBe(0);
      let at = 0;
      for (const line of a.expectBoot) {
        const i = r.out.indexOf(line, at);
        expect(i, `missing "${line}" in\n${r.out}`).toBeGreaterThanOrEqual(0);
        at = i + line.length;
      }
    });
  }
});
