# Entry from the hypervisor to C: PVH start in 32-bit protected mode, then long mode, then kmain.
# The PVH ELF note tells the loader where to jump (XEN_ELFNOTE_PHYS32_ENTRY = 18). On entry the CPU is
# in 32-bit protected mode with flat segments, paging off, interrupts off, EBX = hvm_start_info.
# We identity-map the first 1 GiB with 2 MiB pages, switch to long mode and call kmain.
    .intel_syntax noprefix

    .pushsection .note.Xen, "a", @note
    .balign 4
    .long 4, 4, 18
    .asciz "Xen"
    .balign 4
    .long _start32
    .popsection

    .section .text.boot, "ax", @progbits
    .code32
    .global _start32
_start32:
    cli
    mov esp, offset boot_stack_top

    mov eax, offset pdpt
    or eax, 0x3                         # present | writable
    mov dword ptr [pml4], eax
    mov eax, offset pd
    or eax, 0x3
    mov dword ptr [pdpt], eax

    xor ecx, ecx                        # fill the page directory: 512 entries x 2 MiB
1:  mov eax, ecx
    shl eax, 21
    or eax, 0x83                        # present | writable | page size (2 MiB)
    mov dword ptr [pd + ecx*8], eax
    mov dword ptr [pd + ecx*8 + 4], 0
    inc ecx
    cmp ecx, 512
    jne 1b

    mov eax, offset pml4
    mov cr3, eax
    mov eax, cr4
    or eax, 1 << 5                      # CR4.PAE
    mov cr4, eax
    mov ecx, 0xC0000080                 # IA32_EFER
    rdmsr
    or eax, 1 << 8                      # EFER.LME
    wrmsr
    mov eax, cr0
    or eax, 1 << 31                     # CR0.PG: long mode becomes active (compatibility sub-mode)
    mov cr0, eax

    lgdt [gdt_ptr]
    .byte 0xEA                          # ljmp 0x08:long_mode, spelled as bytes: far jump into the 64-bit code segment
    .long long_mode
    .word 0x08

    .code64
long_mode:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov ss, ax
    xor eax, eax
    mov fs, ax
    mov gs, ax
    mov rsp, offset boot_stack_top
    xor ebp, ebp
    call kmain
2:  hlt
    jmp 2b

    .section .rodata.boot, "a", @progbits
    .balign 8
gdt:
    .quad 0x0000000000000000            # null
    .quad 0x00209A0000000000            # 0x08 code: L=1, present, executable
    .quad 0x0000920000000000            # 0x10 data: present, writable
gdt_end:
gdt_ptr:
    .word gdt_end - gdt - 1
    .long gdt

    .section .bss.boot, "aw", @nobits
    .balign 4096
pml4:           .skip 4096
pdpt:           .skip 4096
pd:             .skip 4096
boot_stack:     .skip 65536
boot_stack_top:
