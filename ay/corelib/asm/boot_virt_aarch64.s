// Bare-metal AArch64 for QEMU `-M virt`: EL1 entry, stack, vector table, PL011 output, one SVC exception, semihosting exit.
// Build: aarch64-linux-gnu-as boot_virt_aarch64.s -o boot.o && aarch64-linux-gnu-ld -nostdlib -Ttext=0x40000000 boot.o -o boot.elf
// Run:   qemu-system-aarch64 -M virt -cpu cortex-a72 -nographic -semihosting -kernel boot.elf
	.equ	UART0, 0x09000000	// PL011 on the virt board
	.equ	UARTFR, 0x18		// flag register: bit 5 = TXFF (transmit FIFO full)

	.text
	.globl	_start
_start:
	adrp	x0, stack_top		// the loader gives us no stack: point SP into .bss
	add	x0, x0, :lo12:stack_top
	mov	sp, x0
	adr	x0, vectors		// 2 KiB-aligned table of 16 entries
	msr	vbar_el1, x0
	isb

	adr	x0, msg_boot
	bl	puts
	mrs	x1, CurrentEL		// bits [3:2] hold the exception level
	lsr	x1, x1, #2
	add	w0, w1, #'0'
	bl	putc
	mov	w0, #'\n'
	bl	putc

	svc	#0			// synchronous exception from EL1 to EL1: vector at VBAR + 0x200
	adr	x0, msg_back
	bl	puts

	mov	x0, #0x18		// semihosting SYS_EXIT; x1 points to {reason, exit status}
	adr	x1, exit_block
	hlt	#0xf000
1:	wfi
	b	1b

putc:					// w0 = byte; clobbers x9, w10
	mov	x9, #UART0
1:	ldr	w10, [x9, #UARTFR]
	tbnz	w10, #5, 1b		// wait while the FIFO is full
	strb	w0, [x9]
	ret

puts:					// x0 = NUL-terminated string; clobbers x9-x12
	stp	x29, x30, [sp, #-16]!
	mov	x29, sp
	mov	x12, x0
1:	ldrb	w0, [x12], #1
	cbz	w0, 2f
	bl	putc
	b	1b
2:	ldp	x29, x30, [sp], #16
	ret

sync_handler:				// ESR_EL1[31:26] is the exception class; 0x15 = SVC from AArch64
	mrs	x0, esr_el1
	lsr	x0, x0, #26
	cmp	x0, #0x15
	b.ne	hang
	adr	x0, msg_svc
	bl	puts
	eret				// back to ELR_EL1, the instruction after the svc
hang:	b	hang

	.macro	ventry target
	.balign	128			// every entry owns 0x80 bytes
	b	\target
	.endm

	.balign	2048
vectors:
	ventry	hang			// 0x000 current EL, SP0: sync
	ventry	hang			// 0x080 irq
	ventry	hang			// 0x100 fiq
	ventry	hang			// 0x180 serror
	ventry	sync_handler		// 0x200 current EL, SPx: sync  <- our SVC lands here
	ventry	hang			// 0x280 irq
	ventry	hang			// 0x300 fiq
	ventry	hang			// 0x380 serror
	.rept	8			// lower EL, AArch64 and AArch32
	ventry	hang
	.endr

	.section .rodata
msg_boot: .asciz "hello from AArch64 EL"
msg_svc:  .asciz "caught svc (ESR_EL1.EC == 0x15)\n"
msg_back: .asciz "returned from the handler, exiting\n"
	.balign	8
exit_block: .quad 0x20026, 0		// ADP_Stopped_ApplicationExit, status 0

	.bss
	.balign	16
	.skip	4096
stack_top:
