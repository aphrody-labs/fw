/* Fw x86-64 Assembly Kernel (Windows x64 & System V AMD64 ABI) */
.text
.globl fw_asm_add_u64
.globl fw_asm_fma_f32

/* uint64_t fw_asm_add_u64(uint64_t a, uint64_t b) */
fw_asm_add_u64:
#if defined(_WIN32) || defined(__MINGW32__) || defined(_MSC_VER)
    leaq (%rcx, %rdx), %rax
#else
    leaq (%rdi, %rsi), %rax
#endif
    ret

/* float fw_asm_fma_f32(float a, float b, float c) -> a * b + c */
fw_asm_fma_f32:
    mulss %xmm1, %xmm0
    addss %xmm2, %xmm0
    ret
