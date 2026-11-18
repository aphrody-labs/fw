; System V AMD64 / Win64 compatible byte-sum kernel (scalar unrolled baseline)
; uint64_t ay_luma_sum_asm(const uint8_t *pixels, size_t len)
section .text
global ay_luma_sum_asm

ay_luma_sum_asm:
%ifdef WIN64
    ; rcx = pixels, rdx = len
    mov r8, rcx
    mov r9, rdx
%else
    ; rdi = pixels, rsi = len
    mov r8, rdi
    mov r9, rsi
%endif
    xor eax, eax
    test r9, r9
    jz .done
.loop:
    movzx r10d, byte [r8]
    add rax, r10
    inc r8
    dec r9
    jnz .loop
.done:
    ret
