#!/usr/bin/env bun
// Boot the bare-metal corelib image in QEMU and compare its serial output with the shared golden transcript.
//   bun tools/corelib-qemu.ts                 build (clang, lld) and boot on this machine (Linux with qemu-system-x86_64)
//   bun tools/corelib-qemu.ts --remote        sync the sources to the build host of ay.toml, run there, fetch the log
//   options: --log PATH (where the serial log is written), --dir PATH (remote work dir, default /srv/xbuild/ay-corelib)
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";

import { buildX86Image } from "./corelib.ts";

const root = resolve(import.meta.dir, "..");
const GOLDEN = "corelib/tests/golden/boot.txt";
/** isa-debug-exit maps the value 0x10 written by the image to process status (0x10 << 1) | 1. */
export const PASS_STATUS = 33;

export function qemuArgs(image: string, log: string): string[] {
  return [
    "qemu-system-x86_64", "-machine", "q35", "-cpu", "qemu64", "-m", "128M", "-display", "none", "-no-reboot",
    "-serial", `file:${log}`, "-device", "isa-debug-exit,iobase=0xf4,iosize=0x04", "-kernel", image,
  ];
}

/** Empty when the serial log equals the golden transcript and QEMU reported a pass; otherwise the reasons. */
export function verdict(status: number, serial: string, golden: string): string[] {
  const out: string[] = [];
  if (status !== PASS_STATUS) out.push(`qemu exit status ${status}, expected ${PASS_STATUS}`);
  if (serial !== golden) {
    const a = serial.split("\n");
    const b = golden.split("\n");
    const at = a.findIndex((l, i) => l !== b[i]);
    out.push(`serial differs from golden at line ${at + 1}: ${JSON.stringify(a[at] ?? "")} vs ${JSON.stringify(b[at] ?? "")}`);
  }
  return out;
}

function sh(cmd: string[], opts: { cwd?: string; stdin?: Uint8Array } = {}): { status: number; out: string } {
  const p = Bun.spawnSync(cmd, { cwd: opts.cwd ?? root, stdin: opts.stdin, stdout: "pipe", stderr: "pipe" });
  return { status: p.exitCode ?? -1, out: p.stdout.toString() + p.stderr.toString() };
}

function arg(name: string): string | undefined {
  const i = process.argv.indexOf(name);
  return i >= 0 ? process.argv[i + 1] : undefined;
}

function runLocal(log: string): number {
  const image = buildX86Image();
  mkdirSync(dirname(log), { recursive: true });
  const qemu = sh(["timeout", "60", ...qemuArgs(image, log)]);
  const serial = existsSync(log) ? readFileSync(log, "utf8") : "";
  const bad = verdict(qemu.status, serial, readFileSync(join(root, GOLDEN), "utf8"));
  for (const b of bad) console.error(`qemu: ${b}`);
  if (bad.length === 0) console.log(`qemu: boot transcript identical (${serial.split("\n").length - 1} lines), exit ${qemu.status}, log ${log}`);
  return bad.length === 0 ? 0 : 1;
}

function runRemote(log: string): number {
  const ay = Bun.TOML.parse(readFileSync(join(root, "ay.toml"), "utf8")) as { hosts: { build: string } };
  const host = ay.hosts.build;
  const dir = arg("--dir") ?? "/srv/xbuild/ay-corelib";
  const files = ["ay.toml", "corelib", "tools/corelib.ts", "tools/corelib-qemu.ts"];
  const tar = Bun.spawnSync(["tar", "czf", "-", ...files], { cwd: root, stdout: "pipe" });
  if (tar.exitCode !== 0) return 1;
  const remoteLog = `${dir}/logs/qemu-boot.log`;
  const remote = `mkdir -p ${dir} && cd ${dir} && tar xzf - && bun tools/corelib-qemu.ts --log ${remoteLog}`;
  const run = sh(["ssh", host, remote], { stdin: new Uint8Array(tar.stdout) });
  console.log(run.out.trim());
  if (run.status !== 0) return 1;
  const fetched = sh(["ssh", host, `cat ${remoteLog}`]);
  mkdirSync(dirname(log), { recursive: true });
  writeFileSync(log, fetched.out);
  console.log(`qemu: ${host}:${remoteLog} copied to ${log}`);
  return 0;
}

if (import.meta.main) {
  const log = resolve(arg("--log") ?? join(root, "target/qemu-boot.log"));
  process.exit(process.argv.includes("--remote") ? runRemote(log) : runLocal(log));
}
