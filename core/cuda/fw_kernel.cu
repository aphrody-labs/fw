// Fw CUDA / NVPTX64 GPU Compute Kernels (sm_80 / sm_89 / sm_90)
// Compile PTX via LLVM Clang: clang --target=nvptx64-nvidia-cuda -x c -O3 -S core/cuda/fw_nvptx.c -o target/lowlevel/fw.ptx
// Or NVCC: nvcc -O3 -arch=sm_89 --ptx core/cuda/fw_kernel.cu -o target/lowlevel/fw.ptx

#ifdef __CUDACC__
extern "C" __global__ void fw_saxpy(int n, float alpha, const float* x, float* y) {
    int idx = blockIdx.x * blockDim.x + threadIdx.x;
    if (idx < n) {
        y[idx] = fmaf(alpha, x[idx], y[idx]);
    }
}

extern "C" __global__ void fw_rmsnorm_row(
    const float* x,
    const float* weight,
    float* out,
    int dim,
    float eps
) {
    extern __shared__ float smem[];
    int tid = threadIdx.x;
    float sum = 0.0f;
    for (int i = tid; i < dim; i += blockDim.x) {
        float v = x[i];
        sum += v * v;
    }
    smem[tid] = sum;
    __syncthreads();
    for (int stride = blockDim.x >> 1; stride > 0; stride >>= 1) {
        if (tid < stride) smem[tid] += smem[tid + stride];
        __syncthreads();
    }
    float inv = rsqrtf(smem[0] / (float)dim + eps);
    for (int i = tid; i < dim; i += blockDim.x) {
        out[i] = x[i] * inv * weight[i];
    }
}
#endif
