import { test, expect, describe } from "bun:test";
import { getuid, getgid, geteuid, getegid, spawnWithIds } from "../src/node-cp.ts";

describe("node-cp: libc uid/gid via FFI", () => {
  test("getuid() returns a number >= 0", () => {
    const uid = getuid();
    expect(typeof uid).toBe("number");
    expect(uid).toBeGreaterThanOrEqual(0);
  });

  test("getgid() returns a number >= 0", () => {
    const gid = getgid();
    expect(typeof gid).toBe("number");
    expect(gid).toBeGreaterThanOrEqual(0);
  });

  test("geteuid() === getuid() (non-setuid context)", () => {
    // On a standard VPS without setuid binaries, euid == uid.
    expect(geteuid()).toBe(getuid());
  });

  test("getegid() === getgid() (non-setuid context)", () => {
    expect(getegid()).toBe(getgid());
  });

  test("spawnWithIds returns an object with uid and gid properties", async () => {
    const proc = spawnWithIds(["echo", "hi"], { stdout: "pipe" });
    expect(proc).toHaveProperty("uid");
    expect(proc).toHaveProperty("gid");
    expect(typeof proc.uid).toBe("number");
    expect(typeof proc.gid).toBe("number");
    expect(proc.uid).toBeGreaterThanOrEqual(0);
    expect(proc.gid).toBeGreaterThanOrEqual(0);
    await proc.exited;
  });

  test("spawnWithIds keeps proc.pid (Bun.spawn still works)", async () => {
    const proc = spawnWithIds(["echo", "hello"], { stdout: "pipe" });
    expect(typeof proc.pid).toBe("number");
    expect(proc.pid).toBeGreaterThan(0);
    await proc.exited;
  });

  test("injected uid/gid match the direct libc values", async () => {
    const proc = spawnWithIds(["true"], { stdout: "pipe" });
    expect(proc.uid).toBe(getuid());
    expect(proc.gid).toBe(getgid());
    await proc.exited;
  });

  test("libc ids match process.getuid() / process.getgid()", () => {
    expect(getuid()).toBe(process.getuid!());
    expect(getgid()).toBe(process.getgid!());
  });

  test("spawnWithIds forwards the spawn options (stdout is readable)", async () => {
    const proc = spawnWithIds(["echo", "forwarded"], { stdout: "pipe" });
    const out = await new Response(proc.stdout as ReadableStream).text();
    expect(out.trim()).toBe("forwarded");
    expect(await proc.exited).toBe(0);
  });
});
