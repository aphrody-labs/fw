/* LLVM NVPTX64 Device-Side C Kernel (compiles directly with clang --target=nvptx64-nvidia-cuda -S -O3) */
void fairwind_nvptx_saxpy(int n, float alpha, const float *x, float *y) {
    for (int i = 0; i < n; ++i) {
        y[i] = alpha * x[i] + y[i];
    }
}

float fairwind_nvptx_dot(int n, const float *a, const float *b) {
    float acc = 0.0f;
    for (int i = 0; i < n; ++i) {
        acc += a[i] * b[i];
    }
    return acc;
}
