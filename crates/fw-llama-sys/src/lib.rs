//! Low-level dynamic bindings for llama.cpp.

#![allow(
    clippy::all,
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    unpredictable_function_pointer_comparisons
)]

/// `ggml-org/llama.cpp` release tag the vendored `include/` headers come from (`bun sync-headers.ts <tag>` updates them).
pub const LLAMA_CPP_TAG: &str = include_str!("../include/LLAMA_CPP_TAG");

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
