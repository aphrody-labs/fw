//! Minimal C ABI library for Fairwind (`bun:ffi` consumer).

const ABI_VERSION: u32 = 1;
const HEALTH_JSON: &[u8] = b"{\"ok\":true,\"crate\":\"fairwind-abi\",\"abiVersion\":1}\0";

#[unsafe(no_mangle)]
pub extern "C" fn fairwind_abi_version() -> u32 {
    ABI_VERSION
}

#[unsafe(no_mangle)]
pub extern "C" fn fairwind_health_json() -> *const u8 {
    HEALTH_JSON.as_ptr()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CStr;

    #[test]
    fn abi_version_is_nonzero() {
        assert_eq!(fairwind_abi_version(), 1);
    }

    #[test]
    fn health_json_is_valid_cstr() {
        let ptr = fairwind_health_json();
        assert!(!ptr.is_null());
        let s = unsafe { CStr::from_ptr(ptr as *const i8) }.to_str().expect("valid UTF-8");
        assert!(s.contains("\"ok\":true"));
        assert!(s.contains("\"abiVersion\":1"));
    }
}
