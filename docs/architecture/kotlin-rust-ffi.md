# Kotlin <-> Rust FFI, Tree-sitter Kotlin, Kotlin LSP & Android CLI Standards

Verified live on `2026-11-19` for `fw` and all `aphrody-labs` repositories.

## 1. Installed Toolchain

- **Android CLI**: `1.1.16919734` (`C:\Users\aphro\AppData\AndroidCLI\android.exe`), official skills installed via `android skills add --all` (including `migrate-xml-views-to-jetpack-compose`, `jetpack-compose-performance`, `adaptive`, `camerax`, `ink-app-builder`, `litert-lm`).
- **JetBrains Official Kotlin LSP**: `v271.2085.0` (`%LOCALAPPDATA%\kotlin-lsp\bin\intellij-server.exe --stdio`, wrapper `kotlin-lsp.cmd`).
- **Gradle**: `9.9.1` with Kotlin `2.4.21` and Microsoft OpenJDK `25.0.5 LTS`.
- **Tree-sitter Kotlin**: `tree-sitter-kotlin-ng = "1.1.0"` compiled directly into Fat-LTO `native/fw-abi` (`fw_abi.dll`) alongside 11 other grammars.

## 2. Bidirectional Kotlin <-> Rust Best Practices

| Strategy | Direction | Target | Version | When to Use |
| --- | --- | --- | --- | --- |
| **JDK 22+/25 Panama FFM** (`java.lang.foreign.*`) | Bidirectional (`downcallHandle` + `upcallStub`) | JVM Desktop/Server, Gradle, `fw-rust-ffi` | JDK 25 LTS | Default for Kotlin/JVM <-> Fat-LTO Rust C ABI (`fw_abi.dll`). Zero JNI glue, `Arena`-scoped deterministic lifetime, direct Rust->Kotlin callbacks via `Linker.upcallStub`. |
| **Mozilla UniFFI** (`uniffi`) | Bidirectional (traits, async, errors) | Android API 21+, KMP (Gobley), iOS | `0.32.2` | Default for Android & Compose Multiplatform shared libraries needing idiomatic Kotlin data classes, sealed errors, coroutines, and callback interfaces. |
| **Direct JNI** (`jni`) | Bidirectional (`JNIEnv` + `native`) | Android NDK, JVM | `0.22.4` | Zero-copy `DirectByteBuffer`, `AHardwareBuffer`, or `ANativeWindow` surface handoff to `wgpu` on Android. |
| **Kotlin/Native `cinterop`** | Bidirectional (`cbindgen` + `staticCFunction`) | iOS, Linux/Win/macOS Native | Kotlin `2.4.21` | Direct LLVM static linking against `libfw_abi.a` without JVM overhead. |

## 3. FFI Safety Rules in `fw`

1. Never unwind a Rust panic across `extern "C"`: `native/fw-abi` compiles with `panic = "abort"` and checks all raw pointers for null before `slice::from_raw_parts`.
2. Always bind native allocations to `Arena.ofConfined().use { ... }` (for single-call buffers and upcall stubs) or `Arena.ofShared()` (for library lifetime, closed via `AutoCloseable`).
3. Keep `extern "C"` signatures flat: `(ptr: *const u8, len: usize, out_ptr: *mut u8, out_cap: usize) -> usize` for JSON/byte buffers, and `Option<extern "C" fn(*const u8, usize) -> i32>` for Rust -> Kotlin upcalls.
4. Pass `--enable-native-access=ALL-UNNAMED` on JDK 25 to avoid restricted-method warnings.
