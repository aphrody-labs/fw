// DWARF to BTF: clang compiles the all-headers unit for the target with every type kept
// (-fno-eliminate-unused-debug-types), pahole -J turns the DWARF into BTF, bpftool dumps it as JSON.
import { join } from "node:path";
import { runOk } from "./exec.ts";
import { buildModel, parseBtfJson, type Model } from "./model.ts";
import { type Tu } from "./tu.ts";

export async function readModel(tu: Tu): Promise<Model> {
  const object = join(tu.workDir, "all.o");
  await runOk(["clang", ...tu.cflags, "-g", "-gdwarf-5", "-fno-eliminate-unused-debug-types", "-c", tu.sourcePath, "-o", object]);
  await runOk(["pahole", "-J", object]);
  const dump = await runOk(["bpftool", "-j", "btf", "dump", "file", object]);
  return buildModel(parseBtfJson(dump.stdout));
}
