#include <asm/x.h>

SYM_FUNC_START(entry_SYSCALL_64)
	call ksys_read
SYM_FUNC_END(entry_SYSCALL_64)
