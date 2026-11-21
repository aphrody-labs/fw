import { describe, expect, test } from "bun:test";
import { existsSync } from "node:fs";
import { totalmem } from "node:os";
import { SyscallError, addressOf, allocAligned, ioctl, syscall } from "../src/runtime/index.ts";
import { hostCommand, kernel as k, skipReason } from "./support.ts";

// Real kernel interfaces through bun:ffi, using only generated numbers, constants and layouts.
if (skipReason) {
  test.skip(`kernel interfaces (skipped: ${skipReason})`, () => {});
} else {
  describe("syscalls", () => {
    test("uname: sysname, release and machine match the running kernel", () => {
      const buffer = allocAligned(k.struct_new_utsname);
      expect(syscall(k.syscalls.uname, buffer)).toBe(0);
      const uname = k.struct_new_utsname.read(buffer);
      expect(uname.sysname).toBe("Linux");
      expect(uname.release).toBe(hostCommand(["uname", "-r"]));
      expect(uname.nodename).toBe(hostCommand(["uname", "-n"]));
      expect(uname.machine).toBe(hostCommand(["uname", "-m"]));
      expect(uname.version).toBe(hostCommand(["uname", "-v"]));
    });

    test("sysinfo: struct decode agrees with os.totalmem and /proc/uptime", async () => {
      const buffer = allocAligned(k.struct_sysinfo);
      expect(syscall(k.syscalls.sysinfo, buffer)).toBe(0);
      const info = k.struct_sysinfo.read(buffer);
      expect(Number(info.totalram) * info.mem_unit).toBe(totalmem());
      expect(info.procs).toBeGreaterThan(0);
      const procUptime = Number((await Bun.file("/proc/uptime").text()).split(" ")[0]);
      expect(Math.abs(Number(info.uptime) - procUptime)).toBeLessThanOrEqual(2);
      expect(info.freeram).toBeLessThanOrEqual(info.totalram);
    });

    test("getrandom fills the buffer, twice differently", () => {
      const first = new Uint8Array(32);
      const second = new Uint8Array(32);
      expect(syscall(k.syscalls.getrandom, first, first.length, k.GRND_NONBLOCK)).toBe(32);
      expect(syscall(k.syscalls.getrandom, second, second.length, 0)).toBe(32);
      expect(first.some((byte) => byte !== 0)).toBe(true);
      expect(Buffer.from(first).equals(Buffer.from(second))).toBe(false);
    });

    test("failures raise SyscallError with the kernel errno", () => {
      const path = Buffer.from("/definitely/not/here\0");
      try {
        syscall(k.syscalls.openat, k.AT_FDCWD, path, k.O_RDONLY, 0);
        throw new Error("openat of a missing path succeeded");
      } catch (error) {
        expect(error).toBeInstanceOf(SyscallError);
        expect((error as SyscallError).errno).toBe(k.ENOENT);
      }
    });

    test("syscall numbers resolve back to their names", () => {
      expect(k.syscallNames[k.syscalls.uname]).toBe("uname");
      expect(k.syscallNames[k.syscalls.getrandom]).toBe("getrandom");
    });
  });

  describe("ioctl", () => {
    test("TIOCSWINSZ then TIOCGWINSZ on a pty master round-trips the generated winsize layout", () => {
      const fd = Number(syscall(k.syscalls.openat, k.AT_FDCWD, Buffer.from("/dev/ptmx\0"), k.O_RDWR | k.O_NOCTTY, 0));
      try {
        const { TIOCGWINSZ, TIOCSWINSZ } = k.ioctls;
        expect(TIOCGWINSZ.arg).toBe(k.struct_winsize);
        const written = { ws_row: 37, ws_col: 121, ws_xpixel: 1000, ws_ypixel: 777 };
        ioctl(fd, TIOCSWINSZ.request, allocWith(written));
        const readBack = allocAligned(k.struct_winsize);
        expect(ioctl(fd, TIOCGWINSZ.request, readBack)).toBe(0);
        expect(k.struct_winsize.read(readBack)).toEqual(written);
      } finally {
        syscall(k.syscalls.close, fd);
      }
    });

    test("TIOCGPTN (an _IOR) returns the pty number as the decoded unsigned int", () => {
      const fd = Number(syscall(k.syscalls.openat, k.AT_FDCWD, Buffer.from("/dev/ptmx\0"), k.O_RDWR | k.O_NOCTTY, 0));
      try {
        const { TIOCGPTN } = k.ioctls;
        expect(TIOCGPTN.size).toBe(4);
        const number = new Uint32Array(1);
        ioctl(fd, TIOCGPTN.request, number);
        ioctl(fd, k.ioctls.TIOCSPTLCK.request, new Int32Array([0]));
        expect(existsSync(`/dev/pts/${number[0]}`)).toBe(true);
      } finally {
        syscall(k.syscalls.close, fd);
      }
    });

    test("an invalid fd raises EBADF", () => {
      try {
        ioctl(-1, k.ioctls.TIOCGWINSZ.request, allocAligned(k.struct_winsize));
        throw new Error("ioctl on fd -1 succeeded");
      } catch (error) {
        expect(error).toBeInstanceOf(SyscallError);
        expect((error as SyscallError).errno).toBe(k.EBADF);
      }
    });
  });

  describe("helpers", () => {
    test("allocAligned returns a buffer aligned to the layout", () => {
      for (const layout of [k.struct_winsize, k.struct_new_utsname, k.struct_sysinfo, k.struct_perf_event_attr]) {
        const buffer = allocAligned(layout);
        expect(buffer.length).toBe(layout.size);
        expect(Number(addressOf(buffer)) % layout.align).toBe(0);
      }
    });
  });
}

function allocWith(value: { ws_row: number; ws_col: number; ws_xpixel: number; ws_ypixel: number }): Uint8Array {
  const buffer = allocAligned(k.struct_winsize);
  k.struct_winsize.write(buffer, value);
  return buffer;
}
