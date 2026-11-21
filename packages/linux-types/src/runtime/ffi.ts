// Linux-only helpers over bun:ffi: libc `syscall`, `ioctl`, buffer addresses and aligned
// allocation. bun:ffi is loaded lazily so importing the package on another OS (to read the types)
// does not fail; the helpers throw a clear error instead.
import type { Layout } from "./layout.ts";

export class SyscallError extends Error {
  constructor(
    readonly errno: number,
    readonly call: string,
  ) {
    super(`${call} failed with errno ${errno}`);
    this.name = "SyscallError";
  }
}

export type SyscallArg = number | bigint | ArrayBuffer | ArrayBufferView | null;

interface Native {
  syscall(...args: (number | bigint)[]): number | bigint;
  ioctl(fd: number, request: number | bigint, argument: number | bigint): number;
  errnoLocation(): number | bigint;
  ptr(view: ArrayBuffer | ArrayBufferView): number;
  readInt32(address: number | bigint): number;
}

let native: Native | undefined;

function load(): Native {
  if (native) return native;
  if (process.platform !== "linux") throw new Error(`linux-types ffi helpers need Linux, this is ${process.platform}`);
  // eslint-disable-next-line @typescript-eslint/no-require-imports
  const ffi = require("bun:ffi") as typeof import("bun:ffi");
  const { FFIType } = ffi;
  const libc = ffi.dlopen("libc.so.6", {
    syscall: { args: [FFIType.i64, FFIType.i64, FFIType.i64, FFIType.i64, FFIType.i64, FFIType.i64, FFIType.i64], returns: FFIType.i64_fast },
    ioctl: { args: [FFIType.i32, FFIType.u64, FFIType.i64], returns: FFIType.i32 },
    __errno_location: { args: [], returns: FFIType.ptr },
  });
  native = {
    syscall: (...args) => libc.symbols.syscall(...(args as [number, number, number, number, number, number, number])),
    ioctl: (fd, request, argument) => libc.symbols.ioctl(fd, request as number, argument as number),
    errnoLocation: () => libc.symbols.__errno_location() as unknown as number,
    ptr: (view) => ffi.ptr(view as Parameters<typeof ffi.ptr>[0]) as unknown as number,
    readInt32: (address) => ffi.read.i32(address as never),
  };
  return native;
}

function currentErrno(ffi: Native): number {
  return ffi.readInt32(ffi.errnoLocation());
}

function toNumeric(ffi: Native, value: SyscallArg): number | bigint {
  if (value === null) return 0;
  if (typeof value === "number" || typeof value === "bigint") return value;
  return ffi.ptr(value);
}

// The address of a buffer as a bigint, the representation pointer members use in struct codecs.
// The buffer must stay alive (referenced) for as long as the kernel may use the address.
export function addressOf(buffer: ArrayBuffer | ArrayBufferView): bigint {
  const ffi = load();
  return BigInt(ffi.ptr(buffer));
}

// Calls Linux syscall `number` through libc `syscall(2)`. Buffers are passed by address. Returns the
// kernel result (a number, or a bigint when it does not fit a double); throws SyscallError with errno
// on failure. errno is read right after the call; libc keeps it per thread.
export function syscall(number: number | bigint, ...args: SyscallArg[]): number | bigint {
  if (args.length > 6) throw new RangeError("a Linux syscall takes at most 6 arguments");
  const ffi = load();
  const padded: (number | bigint)[] = [number];
  for (let index = 0; index < 6; index++) padded.push(toNumeric(ffi, args[index] ?? null));
  const result = ffi.syscall(...padded);
  if (result === -1) throw new SyscallError(currentErrno(ffi), `syscall ${number}`);
  return result;
}

// ioctl(2). `argument` is a buffer (by address), an integer, or omitted.
export function ioctl(fd: number, request: number | bigint, argument: SyscallArg = null): number {
  const ffi = load();
  const result = ffi.ioctl(fd, request, toNumeric(ffi, argument));
  if (result === -1) throw new SyscallError(currentErrno(ffi), `ioctl 0x${request.toString(16)}`);
  return result;
}

// A zeroed buffer for `layout` whose address is a multiple of the record's alignment, so it can be
// handed to the kernel as is. `extra` is the flexible array tail in bytes.
export function allocAligned(layout: Pick<Layout<unknown>, "size" | "align">, extra = 0): Uint8Array {
  const ffi = load();
  const length = layout.size + extra;
  const backing = new Uint8Array(length + layout.align);
  const address = ffi.ptr(backing);
  const shift = (layout.align - (address % layout.align)) % layout.align;
  return backing.subarray(shift, shift + length);
}
