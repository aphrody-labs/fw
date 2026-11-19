#include <stdio.h>
#include <stdint.h>
#include "fairwind_abi.h"

int main(void) {
    uint64_t sum = fairwind_asm_add_u64(19, 23);
    float fma = fairwind_asm_fma_f32(2.5f, 4.0f, 1.5f);
    printf("{\"ok\":true,\"lang\":\"c+asm\",\"add\":%llu,\"fma\":%.2f}\n",
           (unsigned long long)sum, (double)fma);
    return (sum == 42 && fma > 11.4f && fma < 11.6f) ? 0 : 1;
}
