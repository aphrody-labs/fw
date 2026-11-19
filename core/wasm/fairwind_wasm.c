/* Fairwind WebAssembly (wasm32) LTO/AOT Kernel */
#include <stdint.h>

__attribute__((export_name("fairwind_wasm_add")))
uint32_t fairwind_wasm_add(uint32_t a, uint32_t b) {
    return a + b;
}

__attribute__((export_name("fairwind_wasm_dot4")))
float fairwind_wasm_dot4(
    float a0, float a1, float a2, float a3,
    float b0, float b1, float b2, float b3
) {
    return a0 * b0 + a1 * b1 + a2 * b2 + a3 * b3;
}

__attribute__((export_name("fairwind_wasm_fnv1a_u32")))
uint32_t fairwind_wasm_fnv1a_u32(uint32_t seed, uint32_t value) {
    uint32_t h = seed ^ value;
    return h * 16777619u;
}
