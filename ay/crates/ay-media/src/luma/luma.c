#include <stddef.h>
#include <stdint.h>

uint64_t ay_luma_sum_c(const uint8_t *pixels, size_t len) {
    uint64_t acc = 0;
    for (size_t i = 0; i < len; ++i) {
        acc += (uint64_t)pixels[i];
    }
    return acc;
}
