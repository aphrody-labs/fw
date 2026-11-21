//! WOFF2 encoder exported as a wasm32 module with no imports, instantiated by the material.aphrody.com Worker.
//!
//! Calling convention (single-threaded, one result at a time): `alloc` a buffer in linear memory, copy the SFNT in,
//! call `encode`, read `result_len` bytes at the returned pointer, then `release`.

use std::sync::Mutex;
use ttf2woff2::{BrotliQuality, EncodeOptions, encode_with_options};

static RESULT: Mutex<Vec<u8>> = Mutex::new(Vec::new());

/// Allocates `len` bytes of linear memory for the caller to fill.
#[unsafe(no_mangle)]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    let mut buffer = Vec::<u8>::with_capacity(len);
    let pointer = buffer.as_mut_ptr();
    std::mem::forget(buffer);
    pointer
}

/// Frees a buffer returned by `alloc` (`len` is the length that was requested).
///
/// # Safety
/// `pointer` must come from `alloc(len)` and must not be used afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dealloc(pointer: *mut u8, len: usize) {
    drop(unsafe { Vec::from_raw_parts(pointer, 0, len) });
}

/// Compresses the SFNT at `pointer..pointer+len` to WOFF2. Returns a pointer to the result, or null on error;
/// `result_len` then gives its size. `transform` enables the glyf/loca transform.
///
/// # Safety
/// `pointer..pointer+len` must be a readable region of linear memory.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn encode(pointer: *const u8, len: usize, quality: u32, transform: u32) -> *const u8 {
    let input = unsafe { std::slice::from_raw_parts(pointer, len) };
    let options = EncodeOptions {
        quality: BrotliQuality::from(quality.min(11) as u8),
        transform_glyf_loca: transform != 0,
        ..EncodeOptions::default()
    };
    let Ok(mut result) = RESULT.lock() else {
        return std::ptr::null();
    };
    match encode_with_options(input, options) {
        Ok(woff2) => {
            *result = woff2;
            result.as_ptr()
        }
        Err(_) => {
            result.clear();
            std::ptr::null()
        }
    }
}

/// Size in bytes of the last `encode` result.
#[unsafe(no_mangle)]
pub extern "C" fn result_len() -> usize {
    RESULT.lock().map(|result| result.len()).unwrap_or(0)
}
