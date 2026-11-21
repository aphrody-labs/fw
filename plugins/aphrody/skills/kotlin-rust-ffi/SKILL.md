---
name: kotlin-rust-ffi
description: Bidirectional Kotlin-to-Rust and Rust-to-Kotlin FFI best practices using JDK 25 Panama FFM (downcallHandle + upcallStub), Mozilla UniFFI 0.32.2, jni 0.22.4, and Kotlin/Native cinterop.
---

# Bidirectional Kotlin <-> Rust FFI Best Practices

## 1. Selection Matrix

- **Kotlin/JVM <-> Rust Fat-LTO C ABI (`fw_abi.dll`)**: Use **JDK 22+/25 Panama FFM** (`java.lang.foreign.Linker`, `SymbolLookup`, `Arena`, `MemorySegment`) as implemented in `core/kotlin/fw-rust-ffi/src/main/kotlin/com/aphrody/fw/ffi/FwNativeBridge.kt`.
  - **Kotlin -> Rust**: `Linker.nativeLinker().downcallHandle(addr, descriptor)`
  - **Rust -> Kotlin**: `Linker.nativeLinker().upcallStub(methodHandle, descriptor, arena)` passed as `Option<extern "C" fn(*const u8, usize) -> i32>` to Rust (`fw_kotlin_invoke_callback`).
- **Android / Kotlin Multiplatform**: Use **Mozilla UniFFI `0.32.2`** (`uniffi`) for idiomatic Kotlin data classes, sealed errors, `suspend` async functions, and `CallbackInterface` traits.
- **Android NDK Zero-Copy GPU/Buffer Paths**: Use **`jni 0.22.4`** for `DirectByteBuffer`, `AHardwareBuffer`, and `ANativeWindow` -> `wgpu::Surface` handoff.
- **Kotlin/Native (iOS / Native)**: Use **`cbindgen` + `cinterop` + `staticCFunction`** linking directly against Rust `staticlib`.

## 2. Hard Invariants

1. Rust FFI crates use `panic = "abort"` and validate every raw pointer/length before `slice::from_raw_parts`.
2. Scope all temporary native memory and upcall stubs to `Arena.ofConfined().use { ... }`.
3. Run JDK 25 callers with `--enable-native-access=ALL-UNNAMED`.
4. Verify live round-trips via `gradle -p core/kotlin :fw-rust-ffi:test`.
