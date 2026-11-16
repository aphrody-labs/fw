import { expect, test } from "bun:test";
import { PASS_STATUS, qemuArgs, verdict } from "./corelib-qemu.ts";

test("a pass needs QEMU's isa-debug-exit status and an identical serial log", () => {
  expect(verdict(PASS_STATUS, "a\nb\n", "a\nb\n")).toEqual([]);
  expect(verdict(35, "a\nb\n", "a\nb\n")).toEqual(["qemu exit status 35, expected 33"]);
  expect(verdict(PASS_STATUS, "a\nx\n", "a\nb\n")).toEqual(['serial differs from golden at line 2: "x" vs "b"']);
});

test("QEMU is started headless with the serial port on a file and the debug-exit device", () => {
  const args = qemuArgs("img", "out.log");
  expect(args).toContain("file:out.log");
  expect(args).toContain("isa-debug-exit,iobase=0xf4,iosize=0x04");
  expect(args.slice(-2)).toEqual(["-kernel", "img"]);
});
