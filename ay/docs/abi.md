# AY ABI, version 1

<!-- Generated from abi/ay.abi.toml by tools/abi.ts; do not edit. -->

One ABI for every target. All values are 32-bit words: a single i32 signature on WebAssembly, one register width on x86_64, AArch64 and RISC-V. Reference implementation: the freestanding C23 corelib in corelib/src. This header is generated from abi/ay.abi.toml: edit the spec, not the header.

## Syscall numbers. Capability handles are per-task indices; a task starts with its boot capabilities

| Name | Value | Args | Meaning |
|---|---:|---:|---|
| `AY_SYS_VERSION` | 0 | 0 | () -> abi version |
| `AY_SYS_YIELD` | 1 | 0 | () -> 0 |
| `AY_SYS_LOG` | 2 | 3 | (console cap, packed bytes, len 1..4) -> len; bytes little-endian |
| `AY_SYS_MSG_SET` | 3 | 2 | (register 0..3, value) -> 0 |
| `AY_SYS_MSG_GET` | 4 | 1 | (register 0..3) -> value |
| `AY_SYS_SEND` | 5 | 1 | (endpoint cap) -> 0; sends the four message registers |
| `AY_SYS_RECV` | 6 | 1 | (endpoint cap) -> 0; fills the four message registers |
| `AY_SYS_CAP_DERIVE` | 7 | 2 | (cap, rights mask) -> new handle with rights & mask; needs AY_RIGHT_DERIVE |
| `AY_SYS_CAP_DROP` | 8 | 1 | (cap) -> 0 |
| `AY_SYS_FRAME_ALLOC` | 9 | 0 | () -> frame number |
| `AY_SYS_FRAME_FREE` | 10 | 1 | (frame) -> 0; only the owner may free |

## Negative results

| Name | Value | Meaning |
|---|---:|---|
| `AY_EPERM` | -1 | capability lacks the right, or the object is not yours |
| `AY_EBADCAP` | -2 | no such handle, or the wrong kind of object |
| `AY_EINVAL` | -3 | argument out of range |
| `AY_EAGAIN` | -4 | queue full or empty |
| `AY_ENOMEM` | -5 | table or frame pool exhausted |
| `AY_ENOSYS` | -6 | unknown syscall number |

## Capability rights

| Name | Value | Meaning |
|---|---:|---|
| `AY_RIGHT_SEND` | 1 |  |
| `AY_RIGHT_RECV` | 2 |  |
| `AY_RIGHT_DERIVE` | 4 |  |

## Sizes and version

| Name | Value | Meaning |
|---|---:|---|
| `AY_ABI_VERSION` | 1 |  |
| `AY_MSG_WORDS` | 4 |  |

## Functions

| Function | Signature | Meaning |
|---|---|---|
| `ay_syscall` | `int32_t ay_syscall(uint32_t nr, uint32_t a0, uint32_t a1, uint32_t a2);` | The one entry point. Import `ay.syscall` on WebAssembly. |
| `ay_abi_version` | `uint32_t ay_abi_version(void);` | The ABI version the library implements. |
| `ay_rights_allow` | `uint32_t ay_rights_allow(uint32_t have, uint32_t need);` | 1 when a capability holding `have` may do an operation that needs `need`, else 0. |
| `ay_arg_count` | `uint32_t ay_arg_count(uint32_t nr);` | Number of arguments syscall `nr` reads; 0 for an unknown number. |
| `ay_first_free` | `int32_t ay_first_free(uint64_t bits);` | Index of the lowest clear bit, or -1 when all 64 are set: the first free frame. |
| `ay_checksum` | `uint32_t ay_checksum(const uint32_t *words, uintptr_t len);` | Rotate-and-add checksum of a message of `len` words. |
| `ay_sum` | `uint32_t ay_sum(const uint32_t *words, uintptr_t len);` | Wrapping sum of `len` words. |
| `ay_try_acquire` | `uint32_t ay_try_acquire(uint32_t *flag);` | Take the lock word 0 -> 1: 1 when this caller now holds it, else 0. |
