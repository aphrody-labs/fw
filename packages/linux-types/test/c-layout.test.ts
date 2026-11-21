import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { kernel as k, skipReason } from "./support.ts";

// The C compiler is the oracle: sizeof, _Alignof and offsetof of real structs compiled on this machine
// must equal the generated layouts, and bytes written by compiled C (bitfields included) must decode to
// the same values.
const clang = Bun.which("clang");
const reason = skipReason ?? (clang ? undefined : "clang is not installed");

interface Described {
  size: number;
  align: number;
  fields: readonly (readonly [string, number, unknown])[];
}

interface Check {
  c: string;
  layout: Described;
  fields: string[];
}

const checks: Check[] = [
  { c: "struct winsize", layout: k.struct_winsize, fields: ["ws_row", "ws_col", "ws_xpixel", "ws_ypixel"] },
  { c: "struct utsname", layout: k.struct_new_utsname, fields: ["sysname", "nodename", "release", "version", "machine", "domainname"] },
  { c: "struct stat", layout: k.struct_stat, fields: ["st_dev", "st_ino", "st_mode", "st_uid", "st_gid", "st_rdev", "st_size", "st_blksize", "st_blocks"] },
  { c: "struct epoll_event", layout: k.struct_epoll_event, fields: ["events", "data"] },
  { c: "struct iovec", layout: k.struct_iovec, fields: ["iov_base", "iov_len"] },
  { c: "struct sockaddr_in6", layout: k.struct_sockaddr_in6, fields: ["sin6_family", "sin6_port", "sin6_flowinfo", "sin6_addr", "sin6_scope_id"] },
  { c: "struct timespec", layout: k.struct_timespec, fields: ["tv_sec", "tv_nsec"] },
];

const PRELUDE = `#define _GNU_SOURCE
${["sys/types.h", "sys/stat.h", "sys/utsname.h", "sys/epoll.h", "sys/uio.h", "sys/ioctl.h", "netinet/in.h", "time.h", "stddef.h", "stdio.h", "string.h", "linux/perf_event.h"].map((header) => `#include <${header}>`).join("\n")}
`;

function compileAndRun(source: string): string {
  const dir = mkdtempSync(join(tmpdir(), "linux-types-c-"));
  try {
    const compiled = Bun.spawnSync([clang as string, "-x", "c", "-o", join(dir, "probe"), "-"], { stdin: new Blob([source]), stderr: "pipe" });
    if (compiled.exitCode !== 0) throw new Error(`clang failed: ${compiled.stderr.toString()}`);
    const ran = Bun.spawnSync([join(dir, "probe")], { stdout: "pipe" });
    if (ran.exitCode !== 0) throw new Error("probe failed");
    return ran.stdout.toString();
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

// What the compiler says about each struct: "<c> size", "<c> align" and "<c> <field>" to a number.
function compilerFacts(): Map<string, number> {
  const lines = checks.flatMap((check) => [
    `printf("${check.c} size %zu\\n", sizeof(${check.c}));`,
    `printf("${check.c} align %zu\\n", _Alignof(${check.c}));`,
    ...check.fields.map((field) => `printf("${check.c} ${field} %zu\\n", offsetof(${check.c}, ${field}));`),
  ]);
  const output = compileAndRun(`${PRELUDE}int main(void) {\n${lines.join("\n")}\nreturn 0;\n}\n`).trim().split("\n");
  return new Map(output.map((line) => [line.slice(0, line.lastIndexOf(" ")), Number(line.slice(line.lastIndexOf(" ") + 1))]));
}

function mismatches(facts: Map<string, number>, candidates: Check[]): string[] {
  const found: string[] = [];
  for (const check of candidates) {
    if (check.layout.size !== facts.get(`${check.c} size`)) found.push(`${check.c} size ${check.layout.size} != ${facts.get(`${check.c} size`)}`);
    if (check.layout.align !== facts.get(`${check.c} align`)) found.push(`${check.c} align ${check.layout.align} != ${facts.get(`${check.c} align`)}`);
    for (const field of check.fields) {
      const generated = check.layout.fields.find(([name]) => name === field);
      if (!generated) found.push(`${check.c}.${field} missing from the layout`);
      else if (generated[1] !== facts.get(`${check.c} ${field}`)) found.push(`${check.c}.${field} offset ${generated[1]} != ${facts.get(`${check.c} ${field}`)}`);
    }
  }
  return found;
}

if (reason) {
  test.skip(`layouts against the C compiler (skipped: ${reason})`, () => {});
} else {
  describe("layouts against the C compiler", () => {
    const facts = compilerFacts();

    test("sizeof, _Alignof and offsetof of real structs equal the generated layouts", () => {
      expect(facts.size).toBeGreaterThan(40);
      expect(mismatches(facts, checks)).toEqual([]);
    });

    test("a decoder with a wrong offset or size fails the same comparison", () => {
      const broken = checks.map((check) => ({
        ...check,
        layout: {
          size: check.layout.size + (check.c === "struct iovec" ? 8 : 0),
          align: check.layout.align,
          fields: check.layout.fields.map(([name, offset, type]) => [name, check.c === "struct winsize" && name === "ws_col" ? offset + 1 : offset, type] as const),
        },
      }));
      const found = mismatches(facts, broken);
      expect(found).toHaveLength(2);
      expect(found[0]).toStartWith("struct winsize.ws_col offset");
      expect(found[1]).toStartWith("struct iovec size 24");
    });

    test("bitfields set by compiled C decode to the same values", () => {
      const source = `${PRELUDE}
int main(void) {
  struct perf_event_attr a;
  memset(&a, 0, sizeof a);
  a.type = 4; a.size = sizeof a; a.config = 0x1122334455667788ULL;
  a.disabled = 1; a.exclude_user = 1; a.exclude_kernel = 1; a.precise_ip = 2; a.mmap = 1; a.sample_id_all = 1;
  for (size_t i = 0; i < sizeof a; i++) printf("%02x", ((unsigned char *)&a)[i]);
  printf("\\n");
  return 0;
}
`;
      const bytes = Uint8Array.from(compileAndRun(source).trim().match(/../g) as string[], (hex) => Number.parseInt(hex, 16));
      expect(bytes.length).toBe(k.struct_perf_event_attr.size);
      const decoded = k.struct_perf_event_attr.read(bytes);
      expect(decoded).toMatchObject({ type: 4, config: 0x1122334455667788n, disabled: 1n, exclude_user: 1n, exclude_kernel: 1n, precise_ip: 2n, mmap: 1n, sample_id_all: 1n });
      expect(decoded).toMatchObject({ inherit: 0n, pinned: 0n, exclusive: 0n, exclude_hv: 0n, comm: 0n, task: 0n });
    });
  });
}
