// Aphrody SDK: syscall and capability ABI, version 1. Generated from abi/ay.abi.toml by tools/abi.ts; do not edit.
import { dlopen } from "bun:ffi";

// Syscall numbers. Capability handles are per-task indices; a task starts with its boot capabilities.
export const AY_SYS_VERSION = 0;
export const AY_SYS_YIELD = 1;
export const AY_SYS_LOG = 2;
export const AY_SYS_MSG_SET = 3;
export const AY_SYS_MSG_GET = 4;
export const AY_SYS_SEND = 5;
export const AY_SYS_RECV = 6;
export const AY_SYS_CAP_DERIVE = 7;
export const AY_SYS_CAP_DROP = 8;
export const AY_SYS_FRAME_ALLOC = 9;
export const AY_SYS_FRAME_FREE = 10;

// Negative results.
export const AY_EPERM = -1;
export const AY_EBADCAP = -2;
export const AY_EINVAL = -3;
export const AY_EAGAIN = -4;
export const AY_ENOMEM = -5;
export const AY_ENOSYS = -6;

// Capability rights.
export const AY_RIGHT_SEND = 1;
export const AY_RIGHT_RECV = 2;
export const AY_RIGHT_DERIVE = 4;

// Sizes and version.
export const AY_ABI_VERSION = 1;
export const AY_MSG_WORDS = 4;

/** Number of arguments each syscall reads. */
export const SYSCALL_NARGS: Readonly<Record<number, number>> = {
  [AY_SYS_VERSION]: 0,
  [AY_SYS_YIELD]: 0,
  [AY_SYS_LOG]: 3,
  [AY_SYS_MSG_SET]: 2,
  [AY_SYS_MSG_GET]: 1,
  [AY_SYS_SEND]: 1,
  [AY_SYS_RECV]: 1,
  [AY_SYS_CAP_DERIVE]: 2,
  [AY_SYS_CAP_DROP]: 1,
  [AY_SYS_FRAME_ALLOC]: 0,
  [AY_SYS_FRAME_FREE]: 1,
};

/** Symbols of the corelib as `bun:ffi` describes them (the syscall entry is provided by the platform). */
export const symbols = {
  ay_abi_version: { args: [], returns: "u32" },
  ay_rights_allow: { args: ["u32", "u32"], returns: "u32" },
  ay_arg_count: { args: ["u32"], returns: "u32" },
  ay_first_free: { args: ["u64"], returns: "i32" },
  ay_checksum: { args: ["ptr", "usize"], returns: "u32" },
  ay_sum: { args: ["ptr", "usize"], returns: "u32" },
  ay_try_acquire: { args: ["ptr"], returns: "u32" },
} as const;

/** Opens a built corelib (shared library) and binds every symbol. */
export const open = (path: string) => dlopen(path, symbols);
