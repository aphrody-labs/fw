"""Aphrody SDK: syscall and capability ABI, version 1. Generated from abi/ay.abi.toml by tools/abi.ts; do not edit."""
import ctypes

# Syscall numbers. Capability handles are per-task indices; a task starts with its boot capabilities.
AY_SYS_VERSION = 0
AY_SYS_YIELD = 1
AY_SYS_LOG = 2
AY_SYS_MSG_SET = 3
AY_SYS_MSG_GET = 4
AY_SYS_SEND = 5
AY_SYS_RECV = 6
AY_SYS_CAP_DERIVE = 7
AY_SYS_CAP_DROP = 8
AY_SYS_FRAME_ALLOC = 9
AY_SYS_FRAME_FREE = 10

# Negative results.
AY_EPERM = -1
AY_EBADCAP = -2
AY_EINVAL = -3
AY_EAGAIN = -4
AY_ENOMEM = -5
AY_ENOSYS = -6

# Capability rights.
AY_RIGHT_SEND = 1
AY_RIGHT_RECV = 2
AY_RIGHT_DERIVE = 4

# Sizes and version.
AY_ABI_VERSION = 1
AY_MSG_WORDS = 4

SIGNATURES = {
    "ay_abi_version": (ctypes.c_uint32, []),
    "ay_rights_allow": (ctypes.c_uint32, [ctypes.c_uint32, ctypes.c_uint32]),
    "ay_arg_count": (ctypes.c_uint32, [ctypes.c_uint32]),
    "ay_first_free": (ctypes.c_int32, [ctypes.c_uint64]),
    "ay_checksum": (ctypes.c_uint32, [ctypes.POINTER(ctypes.c_uint32), ctypes.c_size_t]),
    "ay_sum": (ctypes.c_uint32, [ctypes.POINTER(ctypes.c_uint32), ctypes.c_size_t]),
    "ay_try_acquire": (ctypes.c_uint32, [ctypes.POINTER(ctypes.c_uint32)]),
}


def bind(lib):
    """Sets restype and argtypes of every corelib function on a loaded library."""
    for name, (restype, argtypes) in SIGNATURES.items():
        fn = getattr(lib, name)
        fn.restype = restype
        fn.argtypes = argtypes
    return lib
