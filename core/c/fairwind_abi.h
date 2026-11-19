#ifndef FAIRWIND_ABI_H
#define FAIRWIND_ABI_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

uint32_t fairwind_abi_version(void);
const uint8_t *fairwind_health_json(void);

size_t fairwind_parse_summary(
    const uint8_t *lang_ptr, size_t lang_len,
    const uint8_t *src_ptr, size_t src_len,
    uint8_t *out_buf, size_t out_cap
);

size_t fairwind_ast_symbols(
    const uint8_t *lang_ptr, size_t lang_len,
    const uint8_t *src_ptr, size_t src_len,
    uint8_t *out_buf, size_t out_cap
);

size_t fairwind_ast_query(
    const uint8_t *lang_ptr, size_t lang_len,
    const uint8_t *src_ptr, size_t src_len,
    const uint8_t *query_ptr, size_t query_len,
    uint8_t *out_buf, size_t out_cap
);

size_t fairwind_inspect_pe(const uint8_t *path_ptr, size_t path_len, uint8_t *out_buf, size_t out_cap);
size_t fairwind_identify_file(const uint8_t *path_ptr, size_t path_len, uint8_t *out_buf, size_t out_cap);
size_t fairwind_disasm_x64(const uint8_t *path_ptr, size_t path_len, size_t max_insns, uint8_t *out_buf, size_t out_cap);
size_t fairwind_inspect_gguf(const uint8_t *path_ptr, size_t path_len, uint8_t *out_buf, size_t out_cap);
size_t fairwind_crypto_digest(const uint8_t *data_ptr, size_t data_len, uint8_t *out_buf, size_t out_cap);
size_t fairwind_git_inspect(const uint8_t *path_ptr, size_t path_len, uint8_t *out_buf, size_t out_cap);
size_t fairwind_ssh_fingerprint(const uint8_t *key_ptr, size_t key_len, uint8_t *out_buf, size_t out_cap);
float fairwind_ml_dot_f32(const float *a, const float *b, size_t len);

/* Assembly kernels (core/asm/fairwind_kernel.S) */
uint64_t fairwind_asm_add_u64(uint64_t a, uint64_t b);
float fairwind_asm_fma_f32(float a, float b, float c);

#ifdef __cplusplus
}
#endif

#endif /* FAIRWIND_ABI_H */
