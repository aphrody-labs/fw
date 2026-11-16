#include "core.h"

AY_API uint32_t ay_abi_version(void) { return AY_ABI_VERSION; }

/// Whether a capability holding `have` may perform an operation that needs `need`.
AY_API uint32_t ay_rights_allow(uint32_t have, uint32_t need) { return (have & need) == need; }

/// Number of arguments each syscall reads (0 for an unknown number).
AY_API uint32_t ay_arg_count(uint32_t nr) {
  switch (nr) {
  case AY_SYS_VERSION:
  case AY_SYS_YIELD:
  case AY_SYS_FRAME_ALLOC:
    return 0;
  case AY_SYS_CAP_DROP:
  case AY_SYS_SEND:
  case AY_SYS_RECV:
  case AY_SYS_MSG_GET:
  case AY_SYS_FRAME_FREE:
    return 1;
  case AY_SYS_MSG_SET:
  case AY_SYS_CAP_DERIVE:
    return 2;
  case AY_SYS_LOG:
    return 3;
  default:
    return 0;
  }
}
