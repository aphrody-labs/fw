# Bare-metal RV64 for QEMU `-M virt`: OpenSBI hands over in S-mode at 0x80200000 with a0 = hart id, a1 = device tree.
# Build: riscv64-linux-gnu-as boot_virt_riscv64.s -o boot.o && riscv64-linux-gnu-ld -N --no-warn-rwx-segments -nostdlib -Ttext=0x80200000 boot.o -o boot.elf
# (`-N` keeps the ELF header out of the loaded segment: QEMU hands the lowest loaded address to OpenSBI as the S-mode entry.)
# Run:   qemu-system-riscv64 -M virt -nographic -bios default -kernel boot.elf
	.option	norvc			# fixed 4-byte instructions, so the trap handler can step over ebreak with sepc += 4
	.equ	UART0, 0x10000000	# 16550 on the virt board: THR at +0, LSR at +5
	.equ	LSR_THRE, 0x20		# transmit holding register empty

	.text
	.globl	_start
_start:
	la	sp, stack_top
	la	t0, trap
	csrw	stvec, t0		# direct mode: every trap enters at `trap`

	la	a0, msg_hart
	call	puts

	la	a0, msg_sbi
	call	puts_sbi

	ebreak				# breakpoint exception, delegated to S-mode by OpenSBI
	la	a0, msg_back
	call	puts

	li	a7, 0x53525354		# SBI SRST extension ("SRST")
	li	a6, 0			# function 0: system_reset
	li	a0, 0			# type 0: shutdown
	li	a1, 0			# reason 0: none
	ecall
1:	wfi
	j	1b

putc:					# a0 = byte; clobbers t0, t1
	li	t0, UART0
1:	lbu	t1, 5(t0)
	andi	t1, t1, LSR_THRE
	beqz	t1, 1b			# wait until the transmitter can take a byte
	sb	a0, 0(t0)
	ret

puts:					# a0 = NUL-terminated string; clobbers t0-t2, a0
	addi	sp, sp, -16
	sd	ra, 0(sp)
	mv	t2, a0
1:	lbu	a0, 0(t2)
	beqz	a0, 2f
	call	putc
	addi	t2, t2, 1
	j	1b
2:	ld	ra, 0(sp)
	addi	sp, sp, 16
	ret

puts_sbi:				# same output through the SBI legacy console, EID 0x01, one byte per ecall
	mv	t2, a0
1:	lbu	a0, 0(t2)
	beqz	a0, 2f
	li	a7, 1			# sbi_console_putchar
	ecall				# S-mode -> M-mode (OpenSBI)
	addi	t2, t2, 1
	j	1b
2:	ret

	.balign	4
trap:					# scause = 3 is a breakpoint
	csrr	t0, scause
	li	t1, 3
	bne	t0, t1, hang
	la	a0, msg_trap
	call	puts
	csrr	t0, sepc
	addi	t0, t0, 4		# skip the 4-byte ebreak
	csrw	sepc, t0
	sret				# back to sepc in the privilege mode saved in sstatus.SPP
hang:	j	hang

	.section .rodata
msg_hart: .asciz "hello from RISC-V S-mode via the 16550\n"
msg_sbi:  .asciz "hello again via SBI console_putchar\n"
msg_trap: .asciz "caught ebreak (scause == 3)\n"
msg_back: .asciz "returned from the handler, shutting down\n"

	.bss
	.balign	16
	.skip	4096
stack_top:
