# Linux RV64 user space, no libc: write(1, msg, len); exit_group(0).
# Syscall ABI: number in a7, arguments in a0..a5, `ecall` traps to the kernel, result in a0.
# Build: riscv64-linux-gnu-as hello_linux_riscv64.s -o hello.o && riscv64-linux-gnu-ld hello.o -o hello
	.globl	_start

	.section .rodata
msg:	.ascii	"hello from RISC-V\n"
	.set	msg_len, . - msg

	.text
_start:
	li	a0, 1			# fd 1 = stdout
	lla	a1, msg			# buffer, PC-relative (auipc + addi)
	li	a2, msg_len		# byte count
	li	a7, 64			# __NR_write
	ecall
	li	a0, 0			# status 0
	li	a7, 94			# __NR_exit_group
	ecall
