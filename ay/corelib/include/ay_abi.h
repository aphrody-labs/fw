/*
 * Aphrody SDK: syscall and capability ABI, version 1.
 *
 * One ABI for every target. All values are 32-bit words: a single i32 signature on WebAssembly,
 * one register width on x86_64, AArch64 and RISC-V. Reference implementation: the freestanding
 * C23 corelib in corelib/src. This header is generated from abi/ay.abi.toml: edit the spec, not the header.
 */
#ifndef AY_ABI_H
#define AY_ABI_H

#include <stdint.h>

/* Syscall numbers. Capability handles are per-task indices; a task starts with its boot capabilities. */
#define AY_SYS_VERSION      0   /* () -> abi version */
#define AY_SYS_YIELD        1   /* () -> 0 */
#define AY_SYS_LOG          2   /* (console cap, packed bytes, len 1..4) -> len; bytes little-endian */
#define AY_SYS_MSG_SET      3   /* (register 0..3, value) -> 0 */
#define AY_SYS_MSG_GET      4   /* (register 0..3) -> value */
#define AY_SYS_SEND         5   /* (endpoint cap) -> 0; sends the four message registers */
#define AY_SYS_RECV         6   /* (endpoint cap) -> 0; fills the four message registers */
#define AY_SYS_CAP_DERIVE   7   /* (cap, rights mask) -> new handle with rights & mask; needs AY_RIGHT_DERIVE */
#define AY_SYS_CAP_DROP     8   /* (cap) -> 0 */
#define AY_SYS_FRAME_ALLOC  9   /* () -> frame number */
#define AY_SYS_FRAME_FREE   10  /* (frame) -> 0; only the owner may free */

/* Negative results. */
#define AY_EPERM    (-1)  /* capability lacks the right, or the object is not yours */
#define AY_EBADCAP  (-2)  /* no such handle, or the wrong kind of object */
#define AY_EINVAL   (-3)  /* argument out of range */
#define AY_EAGAIN   (-4)  /* queue full or empty */
#define AY_ENOMEM   (-5)  /* table or frame pool exhausted */
#define AY_ENOSYS   (-6)  /* unknown syscall number */

/* Capability rights. */
#define AY_RIGHT_SEND    1u
#define AY_RIGHT_RECV    2u
#define AY_RIGHT_DERIVE  4u

/* Sizes and version. */
#define AY_ABI_VERSION  1
#define AY_MSG_WORDS    4

/* The one entry point. On WebAssembly it is the import ("ay", "syscall"); elsewhere the platform provides it.
 * The corelib itself defines it, so its own sources set AY_CORELIB_IMPL and skip this declaration. */
#ifndef AY_CORELIB_IMPL
#ifdef __wasm__
__attribute__((import_module("ay"), import_name("syscall")))
#endif
int32_t  ay_syscall(uint32_t nr, uint32_t a0, uint32_t a1, uint32_t a2);
#endif

/* Pure functions the corelib exports for hosts and tools (no capability needed). */
uint32_t ay_abi_version(void);
uint32_t ay_rights_allow(uint32_t have, uint32_t need);
uint32_t ay_arg_count(uint32_t nr);
int32_t  ay_first_free(uint64_t bits);
uint32_t ay_checksum(const uint32_t *words, uintptr_t len);
uint32_t ay_sum(const uint32_t *words, uintptr_t len);
uint32_t ay_try_acquire(uint32_t *flag);

#endif /* AY_ABI_H */
