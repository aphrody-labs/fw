// child_process extensions: uid/gid through libc FFI.
//
// `node:child_process` in Bun does not expose `proc.gid` or `proc.uid` on child process
// objects (unlike Node.js). This module adds them through dlopen on libc.
//
// Exposed APIs:
//   getuid()   - real UID of the current process       (getuid(2))
//   getgid()   - real GID                              (getgid(2))
//   geteuid()  - effective UID                         (geteuid(2))
//   getegid()  - effective GID                         (getegid(2))
//   spawnWithIds(cmd, opts) - Bun.spawn + uid/gid injection

import { dlopen, FFIType } from "bun:ffi";

const libc = dlopen("libc.so.6", {
  getuid:  { args: [], returns: FFIType.u32 },
  getgid:  { args: [], returns: FFIType.u32 },
  geteuid: { args: [], returns: FFIType.u32 },
  getegid: { args: [], returns: FFIType.u32 },
});

/** Returns the real UID of the current process. */
export function getuid(): number {
  return libc.symbols.getuid();
}

/** Returns the real GID of the current process. */
export function getgid(): number {
  return libc.symbols.getgid();
}

/** Returns the effective UID of the current process. */
export function geteuid(): number {
  return libc.symbols.geteuid();
}

/** Returns the effective GID of the current process. */
export function getegid(): number {
  return libc.symbols.getegid();
}

/**
 * Wraps `Bun.spawn` and injects `uid` and `gid` on the returned object.
 *
 * The child process inherits the credentials of the parent (the getuid/getgid syscalls
 * reflect the current Bun process, which is the expected behaviour when `setuid`/`setgid`
 * are not used).
 */
export function spawnWithIds(
  cmd: string[],
  opts?: Parameters<typeof Bun.spawn>[1],
) {
  const proc = Bun.spawn(cmd, opts);
  return Object.assign(proc, {
    uid: getuid(),
    gid: getgid(),
  });
}
