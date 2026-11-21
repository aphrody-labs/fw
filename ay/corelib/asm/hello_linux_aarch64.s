// Linux AArch64 user space, no libc: write(1, msg, len); exit_group(0).
// Syscall ABI: number in x8, arguments in x0..x5, `svc #0` traps to EL1, result in x0.
// Build: aarch64-linux-gnu-as hello_linux_aarch64.s -o hello.o && aarch64-linux-gnu-ld hello.o -o hello
	.globl	_start

	.section .rodata
msg:	.ascii	"hello from AArch64\n"
	.set	msg_len, . - msg

	.text
_start:
	mov	x0, #1			// fd 1 = stdout
	adr	x1, msg			// buffer, PC-relative (+-1 MiB)
	mov	x2, #msg_len		// byte count
	mov	x8, #64			// __NR_write
	svc	#0
	mov	x0, #0			// status 0
	mov	x8, #94			// __NR_exit_group
	svc	#0
