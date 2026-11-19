//! `fw-compute` — Unified cross-OS (Win32 / Linux UAPI / POSIX / Darwin) and cross-hardware
//! (CUDA sm_89 RTX 4070, WebGPU/WGSL Dawn/wgpu, Vulkan, Metal, CPU AVX2/AVX-512/NEON, TPU PJRT)
//! compute discovery and benchmark execution.

use fw_ggml::{dot_q8_0, quantize_q8_0, GgmlBackendKind, GgmlGraph, GgmlType};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const DEFAULT_WGSL_KERNEL: &str = r#"
@group(0) @binding(0) var<storage, read> a: array<f32>;
@group(0) @binding(1) var<storage, read> b: array<f32>;
@group(0) @binding(2) var<storage, read_write> out: array<f32>;

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    if (i < arrayLength(&out)) {
        out[i] = a[i] * b[i] + a[i];
    }
}
"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareProfile {
    pub os: String,
    pub arch: String,
    pub os_abi: String,
    pub logical_cpus: usize,
    pub simd: Vec<String>,
    pub has_cuda: bool,
    pub has_webgpu_dawn_or_wgpu: bool,
    pub has_vulkan: bool,
    pub has_metal: bool,
    pub has_tpu_pjrt: bool,
    pub recommended_backend: GgmlBackendKind,
    pub wgsl_shader_bytes: usize,
}

fn detect_simd() -> Vec<String> {
    let mut feats = Vec::new();
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        if std::arch::is_x86_feature_detected!("sse4.2") {
            feats.push("sse4.2".into());
        }
        if std::arch::is_x86_feature_detected!("avx2") {
            feats.push("avx2".into());
        }
        if std::arch::is_x86_feature_detected!("fma") {
            feats.push("fma".into());
        }
        if std::arch::is_x86_feature_detected!("avx512f") {
            feats.push("avx512f".into());
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        feats.push("neon".into());
    }
    #[cfg(target_arch = "wasm32")]
    {
        feats.push("wasm-simd128".into());
    }
    feats
}

pub fn detect_hardware_profile() -> HardwareProfile {
    let os = std::env::consts::OS.to_string();
    let arch = std::env::consts::ARCH.to_string();
    let os_abi = match os.as_str() {
        "windows" => "win32+ntfs+conpty+d3d12".to_string(),
        "linux" => "linux-uapi+posix+io_uring+netlink".to_string(),
        "macos" => "darwin+posix+metal+kqueue".to_string(),
        _ => "posix-generic".to_string(),
    };
    let logical_cpus = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
    let simd = detect_simd();

    let has_cuda = if cfg!(windows) {
        Path::new("C:\\Windows\\System32\\nvcuda.dll").exists()
    } else if cfg!(target_os = "linux") {
        Path::new("/dev/nvidia0").exists() || Path::new("/usr/lib/x86_64-linux-gnu/libcuda.so.1").exists()
    } else {
        false
    };
    let has_vulkan = if cfg!(windows) {
        Path::new("C:\\Windows\\System32\\vulkan-1.dll").exists()
    } else if cfg!(target_os = "linux") {
        Path::new("/usr/lib/libvulkan.so.1").exists() || Path::new("/usr/lib/x86_64-linux-gnu/libvulkan.so.1").exists()
    } else {
        false
    };
    let has_metal = cfg!(target_os = "macos") || cfg!(target_os = "ios");
    let has_webgpu_dawn_or_wgpu = has_cuda || has_vulkan || has_metal || cfg!(windows);
    let has_tpu_pjrt = std::env::var("PJRT_DEVICE").is_ok()
        || Path::new("/dev/accel0").exists()
        || Path::new("/usr/lib/libtpu.so").exists();

    let recommended_backend = if has_cuda {
        GgmlBackendKind::CudaSm89
    } else if has_tpu_pjrt {
        GgmlBackendKind::TpuPjrt
    } else if has_webgpu_dawn_or_wgpu {
        GgmlBackendKind::WebGpu
    } else if has_vulkan {
        GgmlBackendKind::Vulkan
    } else if has_metal {
        GgmlBackendKind::Metal
    } else {
        GgmlBackendKind::CpuSimd
    };

    HardwareProfile {
        os,
        arch,
        os_abi,
        logical_cpus,
        simd,
        has_cuda,
        has_webgpu_dawn_or_wgpu,
        has_vulkan,
        has_metal,
        has_tpu_pjrt,
        recommended_backend,
        wgsl_shader_bytes: DEFAULT_WGSL_KERNEL.len(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputeBenchResult {
    pub ok: bool,
    pub backend: GgmlBackendKind,
    pub q8_dot_result: f32,
    pub cgraph_output: Vec<f32>,
    pub elapsed_us: u128,
}

pub fn run_compute_smoke_bench() -> ComputeBenchResult {
    let t0 = std::time::Instant::now();
    let hw = detect_hardware_profile();
    let a = vec![1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
    let b = vec![8.0f32, 7.0, 6.0, 5.0, 4.0, 3.0, 2.0, 1.0];
    let (sa, qa) = quantize_q8_0(&a);
    let (sb, qb) = quantize_q8_0(&b);
    let q8_dot = dot_q8_0(sa, &qa, sb, &qb);

    let mut g = GgmlGraph::new();
    let ta = g.new_tensor_2d("W", GgmlType::F32, 2, 2, vec![1.0, 2.0, 3.0, 4.0]);
    let tb = g.new_tensor_2d("X", GgmlType::F32, 2, 1, vec![2.0, 3.0]);
    let mm = g.mul_mat(ta, tb);
    let norm = g.rms_norm(mm);
    g.schedule_backends(hw.has_cuda, hw.has_webgpu_dawn_or_wgpu, true);
    let ok = g.compute().is_ok();
    ComputeBenchResult {
        ok,
        backend: hw.recommended_backend,
        q8_dot_result: q8_dot,
        cgraph_output: g.tensors[norm].data.clone(),
        elapsed_us: t0.elapsed().as_micros(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hardware_profile_and_compute_bench_succeed() {
        let hw = detect_hardware_profile();
        assert!(hw.logical_cpus >= 1);
        assert!(hw.wgsl_shader_bytes > 100);
        let bench = run_compute_smoke_bench();
        assert!(bench.ok);
        assert!((bench.q8_dot_result - 120.0).abs() < 1.5);
        assert_eq!(bench.cgraph_output.len(), 2);
    }
}
