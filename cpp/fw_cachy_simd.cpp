// CachyOS-optimized C++23 AVX2/FMA SIMD kernels for Fw (Corrosion-rs / C ABI compatible).
// Built with: -O3 -march=x86-64-v3 -flto=auto -std=c++23

#include <cmath>
#include <cstddef>
#include <cstdint>

#if defined(__AVX2__) && defined(__FMA__)
#include <immintrin.h>
#endif

extern "C" {

float fw_cachy_dot_f32(const float* a, const float* b, std::size_t n) noexcept {
    if (!a || !b || n == 0) return 0.0f;
    std::size_t i = 0;
    float sum = 0.0f;
#if defined(__AVX2__) && defined(__FMA__)
    __m256 acc = _mm256_setzero_ps();
    for (; i + 8 <= n; i += 8) {
        __m256 va = _mm256_loadu_ps(a + i);
        __m256 vb = _mm256_loadu_ps(b + i);
        acc = _mm256_fmadd_ps(va, vb, acc);
    }
    alignas(32) float tmp[8];
    _mm256_store_ps(tmp, acc);
    for (int k = 0; k < 8; ++k) sum += tmp[k];
#endif
    for (; i < n; ++i) sum += a[i] * b[i];
    return sum;
}

// In-place magnitude-preserving rank-1 orthogonal projection: W' = W - strength * (W u) u^T
void fw_cachy_abliterate_row_f32(float* row, const float* u, std::size_t dim, float strength, bool preserve_norm) noexcept {
    if (!row || !u || dim == 0) return;
    const float orig_norm = preserve_norm ? std::sqrt(fw_cachy_dot_f32(row, row, dim)) : 0.0f;
    const float proj = fw_cachy_dot_f32(row, u, dim) * strength;
    std::size_t i = 0;
#if defined(__AVX2__) && defined(__FMA__)
    const __m256 vproj = _mm256_set1_ps(proj);
    for (; i + 8 <= dim; i += 8) {
        __m256 vr = _mm256_loadu_ps(row + i);
        __m256 vu = _mm256_loadu_ps(u + i);
        vr = _mm256_fnmadd_ps(vproj, vu, vr);
        _mm256_storeu_ps(row + i, vr);
    }
#endif
    for (; i < dim; ++i) row[i] -= proj * u[i];

    if (preserve_norm && orig_norm > 1e-8f) {
        const float new_norm = std::sqrt(fw_cachy_dot_f32(row, row, dim));
        if (new_norm > 1e-8f) {
            const float scale = orig_norm / new_norm;
            for (std::size_t j = 0; j < dim; ++j) row[j] *= scale;
        }
    }
}

} // extern "C"
