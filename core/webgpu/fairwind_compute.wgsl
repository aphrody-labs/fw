// Fairwind WebGPU WGSL Compute Shaders (GEMM / RMSNorm / Softmax)

struct Params {
    dim: u32,
    eps: f32,
    alpha: f32,
    _pad: u32,
};

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read> input_x: array<f32>;
@group(0) @binding(2) var<storage, read> weight: array<f32>;
@group(0) @binding(3) var<storage, read_write> output_y: array<f32>;

var<workgroup> shared_acc: array<f32, 256>;

@compute @workgroup_size(256)
fn fairwind_saxpy(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    if (i < params.dim) {
        output_y[i] = fma(params.alpha, input_x[i], weight[i]);
    }
}

@compute @workgroup_size(256)
fn fairwind_rmsnorm(@builtin(local_invocation_id) lid: vec3<u32>) {
    let tid = lid.x;
    var sum: f32 = 0.0;
    var i: u32 = tid;
    loop {
        if (i >= params.dim) { break; }
        let v = input_x[i];
        sum = fma(v, v, sum);
        i = i + 256u;
    }
    shared_acc[tid] = sum;
    workgroupBarrier();

    var stride: u32 = 128u;
    loop {
        if (stride == 0u) { break; }
        if (tid < stride) {
            shared_acc[tid] = shared_acc[tid] + shared_acc[tid + stride];
        }
        workgroupBarrier();
        stride = stride >> 1u;
    }

    let inv = inverseSqrt(shared_acc[0] / f32(params.dim) + params.eps);
    var j: u32 = tid;
    loop {
        if (j >= params.dim) { break; }
        output_y[j] = input_x[j] * inv * weight[j];
        j = j + 256u;
    }
}
