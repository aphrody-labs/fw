//! Low-level Rust + C + NASM x86_64 media kernels.

unsafe extern "C" {
    fn ay_luma_sum_c(pixels: *const u8, len: usize) -> u64;
    #[cfg(has_nasm_simd)]
    fn ay_luma_sum_asm(pixels: *const u8, len: usize) -> u64;
}

/// Computes the sum of 8-bit luma plane bytes using the C kernel.
#[must_use]
pub fn luma_sum_c(pixels: &[u8]) -> u64 {
    // SAFETY: `pixels.as_ptr()` is valid for `pixels.len()` bytes for the duration of the call.
    unsafe { ay_luma_sum_c(pixels.as_ptr(), pixels.len()) }
}

/// Computes the sum of 8-bit luma plane bytes using the NASM x86_64 kernel when available,
/// falling back to the C kernel on other targets.
#[must_use]
pub fn luma_sum_asm(pixels: &[u8]) -> u64 {
    #[cfg(has_nasm_simd)]
    {
        // SAFETY: `pixels.as_ptr()` is valid for `pixels.len()` bytes for the duration of the call.
        unsafe { ay_luma_sum_asm(pixels.as_ptr(), pixels.len()) }
    }
    #[cfg(not(has_nasm_simd))]
    {
        luma_sum_c(pixels)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c_and_asm_luma_sum_match() {
        let data: Vec<u8> = (0..=255).collect();
        let expected: u64 = data.iter().map(|&b| u64::from(b)).sum();
        assert_eq!(luma_sum_c(&data), expected);
        assert_eq!(luma_sum_asm(&data), expected);
    }
}
