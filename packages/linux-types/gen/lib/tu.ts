// Builds the all-headers translation unit for one kernel headers_install tree.
//
// The sanitized UAPI headers are libc-free except for a handful of includes (sys/socket.h,
// time.h, ...). Those are satisfied by the stub directory below, which only forwards to kernel
// headers or declares the few libc names the kernel headers expect (struct sockaddr, size_t
// users, BSD integer names). Types and macros that come from the stubs are listed in STUB_TYPES
// and are never emitted as kernel constants.
import { mkdir, readdir, writeFile } from "node:fs/promises";
import { join, relative } from "node:path";
import { type Arch } from "./arch.ts";
import { parallelMap, run, runOk } from "./exec.ts";

export const STUB_FILES: Record<string, string> = {
  "sys/socket.h": `#pragma once
#include <linux/socket.h>
typedef unsigned short sa_family_t;
struct sockaddr { sa_family_t sa_family; char sa_data[14]; };
struct sockaddr_storage { sa_family_t ss_family; char __ss_padding[128 - sizeof(sa_family_t) - sizeof(unsigned long)]; unsigned long __ss_align; };
#define MSG_FIN 0x200
`,
  "time.h": "#pragma once\n#include <linux/time.h>\ntypedef long time_t;\n",
  "sys/time.h": "#pragma once\n#include <time.h>\n",
  "sys/ioctl.h": "#pragma once\n#include <linux/ioctl.h>\n",
  "sys/types.h": `#pragma once
#include <linux/types.h>
#include <stdint.h>
typedef unsigned char u_char; typedef unsigned short u_short; typedef unsigned int u_int; typedef unsigned long u_long;
`,
  "stdint.h": `#pragma once
typedef __INT8_TYPE__ int8_t; typedef __INT16_TYPE__ int16_t; typedef __INT32_TYPE__ int32_t; typedef __INT64_TYPE__ int64_t;
typedef __UINT8_TYPE__ uint8_t; typedef __UINT16_TYPE__ uint16_t; typedef __UINT32_TYPE__ uint32_t; typedef __UINT64_TYPE__ uint64_t;
`,
  "string.h": `#pragma once
#include <stddef.h>
void *memset(void *, int, size_t); void *memcpy(void *, const void *, size_t); int memcmp(const void *, const void *, size_t); size_t strlen(const char *);
`,
  "stdlib.h": "#pragma once\n",
  "unistd.h": "#pragma once\n",
  "netinet/in.h": "#pragma once\n#include <linux/in.h>\n#include <linux/in6.h>\n",
  "netinet/if_ether.h": "#pragma once\n#include <linux/if_ether.h>\n",
};

// Records declared by the libc stubs, not by the kernel; still needed because kernel structs embed them.
export const STUB_TYPES = new Set(["struct sockaddr", "struct sockaddr_storage"]);

export interface Tu {
  workDir: string;
  includeDir: string;
  cflags: string[];
  headers: string[];
  excluded: { header: string; reason: string }[];
  sourcePath: string;
  stubDir: string;
}

async function listHeaders(includeDir: string): Promise<string[]> {
  const entries = await readdir(includeDir, { recursive: true, withFileTypes: true });
  return entries
    .filter((entry) => entry.isFile() && entry.name.endsWith(".h"))
    .map((entry) => relative(includeDir, join(entry.parentPath, entry.name)).replaceAll("\\", "/"))
    .sort();
}

// Arch-variant and generic fallback headers are placed last: when a fallback conflicts with the
// header that really applies to the arch, the fallback is the one that gets pruned.
export const VARIANT = /^asm-generic\/|_(32|64|x32)\.h$|^linux\/byteorder\//;

export async function buildTu(headersDir: string, arch: Arch, workDir: string): Promise<Tu> {
  const includeDir = join(headersDir, "include");
  const stubDir = join(workDir, "stub");
  for (const [path, body] of Object.entries(STUB_FILES)) {
    const file = join(stubDir, path);
    await mkdir(join(file, ".."), { recursive: true });
    await writeFile(file, body);
  }
  const resource = (await runOk(["clang", "-print-resource-dir"])).stdout.trim();
  const cflags = [
    `--target=${arch.triple}`,
    "-nostdinc",
    `-I${includeDir}`,
    "-isystem",
    stubDir,
    "-isystem",
    join(resource, "include"),
    "-include",
    "stddef.h",
    "-include",
    "stdint.h",
    "-include",
    "sys/socket.h",
    "-include",
    "time.h",
    "-w",
  ];

  const all = await listHeaders(includeDir);
  const excluded: Tu["excluded"] = [];
  const standalone = await parallelMap(all, async (header) => {
    const result = await run(["clang", ...cflags, "-fsyntax-only", "-x", "c", "-"], { stdin: `#include <${header}>\n` });
    if (result.code === 0) return { header, reason: "" };
    const first = result.stderr.split("\n").find((line) => line.includes("error")) ?? "";
    return { header, reason: (/error: (.*)$/.exec(first)?.[1] ?? "does not compile").slice(0, 160) };
  });
  const usable: string[] = [];
  for (const { header, reason } of standalone) {
    if (reason === "") usable.push(header);
    else excluded.push({ header, reason: `standalone: ${reason}` });
  }

  const ordered = [...usable.filter((h) => !VARIANT.test(h)), ...usable.filter((h) => VARIANT.test(h))];
  const sourcePath = join(workDir, "all.c");
  let current = ordered;
  for (let round = 0; ; round++) {
    await writeFile(sourcePath, current.map((header) => `#include <${header}>\n`).join(""));
    const result = await run(["clang", ...cflags, "-fsyntax-only", "-ferror-limit=0", sourcePath]);
    if (result.code === 0) break;
    const culprits = new Set<number>();
    for (const match of result.stderr.matchAll(/In file included from [^\n]*all\.c:(\d+):/g)) culprits.add(Number(match[1]));
    if (culprits.size === 0 || round > 50) {
      throw new Error(`all-headers translation unit does not compile and no culprit header was found:\n${result.stderr.slice(0, 3000)}`);
    }
    const dropped = [...culprits].map((line) => ({ line, header: current[line - 1] as string }));
    for (const { header } of dropped) excluded.push({ header, reason: "conflicts with an earlier header in the combined unit" });
    const drop = new Set(dropped.map((entry) => entry.header));
    current = current.filter((header) => !drop.has(header));
  }
  excluded.sort((a, b) => a.header.localeCompare(b.header));
  return { workDir, includeDir, cflags, headers: current, excluded, sourcePath, stubDir };
}
