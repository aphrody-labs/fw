// C functions compiled at run time by TinyCC through bun:ffi `cc`.
// No build step, no .so: just C source linked in memory.
//
// Shows that bun:cc can serve as a light native bridge to C code dropped inline into a
// project (for hot paths, parsers, encoders).

#include <string.h>
#include <stdint.h>

// FNV-1a 64-bit: fast non-cryptographic hash, useful for deduplication.
uint64_t fnv1a64(const unsigned char* data, size_t len) {
    uint64_t h = 0xcbf29ce484222325ULL;
    for (size_t i = 0; i < len; i++) {
        h ^= data[i];
        h *= 0x100000001b3ULL;
    }
    return h;
}

// djb2: classic string hash.
uint32_t djb2(const unsigned char* data, size_t len) {
    uint32_t h = 5381;
    for (size_t i = 0; i < len; i++) {
        h = ((h << 5) + h) + data[i];  // h * 33 + c
    }
    return h;
}

// XOR sum of the bytes: like `cksum` but on 32 bits.
uint32_t xor_sum(const unsigned char* data, size_t len) {
    uint32_t s = 0;
    for (size_t i = 0; i < len; i++) s ^= data[i];
    return s;
}

// Counts the bits set to 1 in the buffer (aggregate popcount).
int popcount_buf(const unsigned char* data, size_t len) {
    int total = 0;
    for (size_t i = 0; i < len; i++) {
        unsigned char b = data[i];
        b = b - ((b >> 1) & 0x55);
        b = (b & 0x33) + ((b >> 2) & 0x33);
        total += (b + (b >> 4)) & 0x0f;
    }
    return total;
}
