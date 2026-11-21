import { describe, expect, test } from "bun:test";
import { bits, bytes, flex, layout } from "../src/runtime/layout.ts";
import { ioctlDef } from "../src/runtime/ioctl.ts";
import * as x86_64 from "../src/x86_64/index.ts";

// Codec behaviour on the generated x86_64 layouts; no kernel calls, so this runs on any host.
describe("struct codecs", () => {
  test("winsize round trip with exact little-endian bytes", () => {
    const { struct_winsize } = x86_64;
    const encoded = struct_winsize.encode({ ws_row: 24, ws_col: 80, ws_xpixel: 640, ws_ypixel: 384 });
    expect([...encoded]).toEqual([24, 0, 80, 0, 0x80, 2, 0x80, 1]);
    expect(struct_winsize.read(encoded)).toEqual({ ws_row: 24, ws_col: 80, ws_xpixel: 640, ws_ypixel: 384 });
    expect(struct_winsize.size).toBe(8);
  });

  test("offset reads and writes inside a larger buffer", () => {
    const { struct_winsize } = x86_64;
    const buffer = new Uint8Array(32);
    struct_winsize.write(buffer, { ws_row: 1, ws_col: 2, ws_xpixel: 3, ws_ypixel: 4 }, 10);
    expect(struct_winsize.read(buffer, 10).ws_ypixel).toBe(4);
    expect(buffer[10]).toBe(1);
    expect(buffer[9]).toBe(0);
  });

  test("64-bit members are bigint, packed structs have no padding", () => {
    const { struct_epoll_event } = x86_64;
    expect(struct_epoll_event.size).toBe(12);
    const encoded = struct_epoll_event.encode({ events: 1, data: 0xfedcba9876543210n });
    expect(struct_epoll_event.read(encoded)).toEqual({ events: 1, data: 0xfedcba9876543210n });
    expect(encoded[4]).toBe(0x10);
  });

  test("char arrays are NUL-terminated strings and refuse overflow", () => {
    const { struct_new_utsname } = x86_64;
    const encoded = struct_new_utsname.encode({ sysname: "Linux", release: "7.3.5" });
    expect(struct_new_utsname.read(encoded)).toMatchObject({ sysname: "Linux", release: "7.3.5", machine: "" });
    expect(() => struct_new_utsname.encode({ sysname: "x".repeat(66) })).toThrow(RangeError);
  });

  test("bitfields read and write without touching their neighbours", () => {
    const { struct_perf_event_attr } = x86_64;
    const buffer = struct_perf_event_attr.zeroed();
    struct_perf_event_attr.write(buffer, { inherit: 1n, exclude_kernel: 1n });
    const decoded = struct_perf_event_attr.read(buffer);
    expect(decoded.inherit).toBe(1n);
    expect(decoded.exclude_kernel).toBe(1n);
    expect(decoded.disabled).toBe(0n);
    expect(decoded.pinned).toBe(0n);
    struct_perf_event_attr.write(buffer, { inherit: 0n });
    expect(struct_perf_event_attr.read(buffer).exclude_kernel).toBe(1n);
    expect(struct_perf_event_attr.read(buffer).inherit).toBe(0n);
  });

  test("anonymous union members are _anonN keys; unions write any subset", () => {
    const { struct_perf_event_attr } = x86_64;
    const buffer = struct_perf_event_attr.zeroed();
    struct_perf_event_attr.write(buffer, { _anon0: { sample_period: 4000n } });
    expect(struct_perf_event_attr.read(buffer)._anon0).toEqual({ sample_period: 4000n, sample_freq: 4000n });
  });

  test("signed bitfields sign-extend", () => {
    const signed = layout<{ a: number; b: number }>("test", 2, 1, [
      ["a", 0, bits("i32", 0, 3)],
      ["b", 0, bits("i32", 3, 5)],
    ]);
    const buffer = signed.encode({ a: -2, b: -16 });
    expect(signed.read(buffer)).toEqual({ a: -2, b: -16 });
  });

  test("flexible array member reads what fits after it", () => {
    const record = layout<{ n: number; items: number[] }>("flexible", 4, 4, [
      ["n", 0, "u32"],
      ["items", 4, flex("u16")],
    ]);
    const buffer = record.zeroed(6);
    record.write(buffer, { n: 3, items: [7, 8, 9] });
    expect(record.read(buffer)).toEqual({ n: 3, items: [7, 8, 9] });
    expect(record.read(buffer.subarray(0, 4)).items).toEqual([]);
  });

  test("raw byte members and nested arrays", () => {
    const record = layout<{ raw: Uint8Array; grid: number[][] }>("mixed", 20, 1, [
      ["raw", 0, bytes(4)],
      ["grid", 4, [["u16", 2], 4]],
    ]);
    const buffer = record.encode({ raw: new Uint8Array([1, 2, 3, 4]), grid: [[1, 2], [3, 4], [5, 6], [7, 8]] });
    expect(record.read(buffer)).toEqual({ raw: new Uint8Array([1, 2, 3, 4]), grid: [[1, 2], [3, 4], [5, 6], [7, 8]] });
  });

  test("a DataView and an offset view over a larger ArrayBuffer decode the same", () => {
    const { struct_winsize } = x86_64;
    const backing = new ArrayBuffer(16);
    struct_winsize.write(backing, { ws_row: 9, ws_col: 8, ws_xpixel: 7, ws_ypixel: 6 }, 4);
    expect(struct_winsize.read(new DataView(backing, 4, 8))).toEqual({ ws_row: 9, ws_col: 8, ws_xpixel: 7, ws_ypixel: 6 });
    expect(struct_winsize.read(new Uint8Array(backing, 4, 8))).toEqual({ ws_row: 9, ws_col: 8, ws_xpixel: 7, ws_ypixel: 6 });
  });
});

describe("generated tables", () => {
  test("ioctl numbers are decoded and typed by their argument struct", () => {
    const { ioctls, struct_winsize } = x86_64;
    expect(ioctls.TIOCGWINSZ.request).toBe(0x5413);
    expect(ioctls.TIOCGWINSZ.arg).toBe(struct_winsize);
    expect(ioctls.TIOCGPTN).toMatchObject({ dir: 2, type: 0x54, nr: 0x30, size: 4, argType: "unsigned int", arg: null });
    expect(ioctls.TIOCGPTN.request).toBe(0x80045430);
    expect(ioctls.FS_IOC_GETFLAGS.argType).toBe("long");
  });

  test("ioctlDef resolves its argument lazily", () => {
    const made = ioctlDef(1, 0, 0, 1, 0, "struct x", () => x86_64.struct_winsize);
    expect(made.arg).toBe(x86_64.struct_winsize);
  });

  test("syscall tables map both ways and constants are exact", () => {
    const { syscalls, syscallNames, O_CREAT, AT_FDCWD, BPF_F_INDEX_MASK } = x86_64;
    expect(syscalls.read).toBe(0);
    expect(syscalls.uname).toBe(63);
    expect(syscallNames[63]).toBe("uname");
    expect(O_CREAT).toBe(0o100);
    expect(AT_FDCWD).toBe(-100);
    expect(BPF_F_INDEX_MASK).toBe(0xffffffff);
    expect(x86_64.enum_bpf_prog_type.BPF_PROG_TYPE_KPROBE).toBe(2);
    expect(x86_64.BPF_PROG_TYPE_KPROBE).toBe(2);
  });

  test("aarch64 differs from x86_64 where the kernel does", () => {
    const aarch64 = require("../src/aarch64/index.ts") as typeof import("../src/aarch64/index.ts");
    expect(aarch64.syscalls.read).toBe(63);
    expect(aarch64.syscalls.uname).toBe(160);
    expect(aarch64.struct_stat.size).toBe(128);
    expect(x86_64.struct_stat.size).toBe(144);
    expect(aarch64.O_DIRECTORY).toBe(0o40000);
    expect(x86_64.O_DIRECTORY).toBe(0o200000);
  });
});
