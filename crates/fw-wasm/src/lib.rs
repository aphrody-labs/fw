//! `fw-wasm` — Pure zero-dependency `wasm32-unknown-unknown` & `wasm32-wasip2` compute kernel
//! shared by Cloudflare Workers (`cloudflare/worker.ts`) and End-User Browser WebGPU/FPU peers.

#[no_mangle]
pub extern "C" fn fw_wasm_abi_version() -> u32 {
    3
}

/// Calibrated System-1 logistic decision (`gemini-decision-1` noul probability in [0.01, 0.99]).
#[no_mangle]
pub extern "C" fn fw_wasm_decision_noul(logit: f32) -> f32 {
    let p = 1.0 / (1.0 + (-logit).exp());
    p.clamp(0.01, 0.99)
}

/// Quantized Q8_0 4-lane dot product for End-User Browser FPU / Wasm SIMD128 drafting.
#[no_mangle]
pub extern "C" fn fw_wasm_q8_dot4(
    a0: i32,
    a1: i32,
    a2: i32,
    a3: i32,
    b0: i32,
    b1: i32,
    b2: i32,
    b3: i32,
    scale_ab: f32,
) -> f32 {
    let acc = a0 * b0 + a1 * b1 + a2 * b2 + a3 * b3;
    (acc as f32) * scale_ab
}

/// Encode Wayland wire protocol header `(size << 16) | opcode` directly in Wasm32 (zero libwayland C deps).
#[no_mangle]
pub extern "C" fn fw_wasm_wayland_word(size: u32, opcode: u32) -> u32 {
    ((size & 0xFFFF) << 16) | (opcode & 0xFFFF)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wasm_exports_compute_accurately() {
        assert_eq!(fw_wasm_abi_version(), 3);
        let p = fw_wasm_decision_noul(0.0);
        assert!((p - 0.5).abs() < 1e-6);
        let d = fw_wasm_q8_dot4(1, 2, 3, 4, 4, 3, 2, 1, 0.5);
        assert!((d - 10.0).abs() < 1e-6);
    }
}
