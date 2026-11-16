# Linux x86-64 user space, no libc: write(1, msg, len); exit_group(0).
# Syscall ABI: number in rax, arguments in rdi rsi rdx r10 r8 r9, `syscall` clobbers rcx and r11, result in rax.
# Build: as hello_linux_x86_64.s -o hello.o && ld hello.o -o hello
	.intel_syntax noprefix
	.globl	_start

	.section .rodata
msg:	.ascii	"hello from x86-64\n"
	.set	msg_len, . - msg

	.text
_start:
	mov	eax, 1			# __NR_write
	mov	edi, 1			# fd 1 = stdout
	lea	rsi, [rip + msg]	# buffer, RIP-relative
	mov	edx, offset msg_len	# byte count; without `offset`, Intel syntax reads memory at msg_len
	syscall
	mov	eax, 231		# __NR_exit_group
	xor	edi, edi		# status 0
	syscall
