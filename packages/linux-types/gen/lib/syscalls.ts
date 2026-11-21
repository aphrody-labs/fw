// Syscall numbers of the arch's native ABI: the __NR_* macros that `#include <asm/unistd.h>`
// defines, evaluated by the compiler (x86_64: unistd_64.h; aarch64: asm-generic/unistd.h with the
// arch's __ARCH_WANT_* selection). The combined all-headers unit is not used here because the
// 32-bit and x32 tables define the same names with different numbers.
import { writeFile } from "node:fs/promises";
import { join } from "node:path";
import { evaluate } from "./evaluate.ts";
import { readMacros } from "./preprocess.ts";
import { type Tu } from "./tu.ts";

export interface SyscallTable {
  syscalls: { name: string; number: number }[];
  // Number of entries the kernel header declares (__NR_syscalls), when it does.
  declaredCount: number | null;
}

// __NR_ names that are table bookkeeping, not syscalls.
const NOT_SYSCALLS = new Set(["syscalls", "arch_specific_syscall"]);

export async function collectSyscalls(tu: Tu): Promise<SyscallTable> {
  const source = join(tu.workDir, "unistd.c");
  await writeFile(source, "#include <asm/unistd.h>\n");
  const macros = await readMacros(tu.cflags, source, tu.includeDir, tu.stubDir);
  const wanted = [...macros.objectLike.values()].filter((macro) => /^__NR_[A-Za-z0-9_]+$/.test(macro.name));
  const results = await evaluate(tu.cflags, source, wanted.map((macro) => ({ key: macro.name, expr: macro.name })), tu.workDir, "syscalls");
  const syscalls: SyscallTable["syscalls"] = [];
  let declaredCount: number | null = null;
  for (const macro of wanted) {
    const result = results.get(macro.name);
    if (!result) continue;
    const name = macro.name.slice("__NR_".length);
    if (name === "syscalls") declaredCount = Number(result.value);
    if (NOT_SYSCALLS.has(name)) continue;
    syscalls.push({ name, number: Number(result.value) });
  }
  syscalls.sort((a, b) => a.number - b.number || a.name.localeCompare(b.name));
  return { syscalls, declaredCount };
}
