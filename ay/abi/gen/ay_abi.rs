//! Aphrody SDK: syscall and capability ABI, version 1. Generated from abi/ay.abi.toml by tools/abi.ts; do not edit.

// Syscall numbers. Capability handles are per-task indices; a task starts with its boot capabilities.
pub const AY_SYS_VERSION: u32 = 0;
pub const AY_SYS_YIELD: u32 = 1;
pub const AY_SYS_LOG: u32 = 2;
pub const AY_SYS_MSG_SET: u32 = 3;
pub const AY_SYS_MSG_GET: u32 = 4;
pub const AY_SYS_SEND: u32 = 5;
pub const AY_SYS_RECV: u32 = 6;
pub const AY_SYS_CAP_DERIVE: u32 = 7;
pub const AY_SYS_CAP_DROP: u32 = 8;
pub const AY_SYS_FRAME_ALLOC: u32 = 9;
pub const AY_SYS_FRAME_FREE: u32 = 10;

// Negative results.
pub const AY_EPERM: i32 = -1;
pub const AY_EBADCAP: i32 = -2;
pub const AY_EINVAL: i32 = -3;
pub const AY_EAGAIN: i32 = -4;
pub const AY_ENOMEM: i32 = -5;
pub const AY_ENOSYS: i32 = -6;

// Capability rights.
pub const AY_RIGHT_SEND: u32 = 1;
pub const AY_RIGHT_RECV: u32 = 2;
pub const AY_RIGHT_DERIVE: u32 = 4;

// Sizes and version.
pub const AY_ABI_VERSION: u32 = 1;
pub const AY_MSG_WORDS: u32 = 4;

#[cfg_attr(target_arch = "wasm32", link(wasm_import_module = "ay"))]
unsafe extern "C" {
    #[cfg_attr(target_arch = "wasm32", link_name = "syscall")]
    pub fn ay_syscall(nr: u32, a0: u32, a1: u32, a2: u32) -> i32;
}

unsafe extern "C" {
    pub fn ay_abi_version() -> u32;
    pub fn ay_rights_allow(have: u32, need: u32) -> u32;
    pub fn ay_arg_count(nr: u32) -> u32;
    pub fn ay_first_free(bits: u64) -> i32;
    pub fn ay_checksum(words: *const u32, len: usize) -> u32;
    pub fn ay_sum(words: *const u32, len: usize) -> u32;
    pub fn ay_try_acquire(flag: *mut u32) -> u32;
}
