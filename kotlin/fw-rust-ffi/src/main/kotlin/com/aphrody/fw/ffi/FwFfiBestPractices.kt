package com.aphrody.fw.ffi

import kotlinx.serialization.Serializable

@Serializable
data class FfiStrategy(
    val name: String,
    val direction: String,
    val targets: List<String>,
    val crateOrApi: String,
    val version: String,
    val zeroCopy: Boolean,
    val whenToUse: String,
)

/**
 * Authoritative Aphrody Labs Kotlin <-> Rust FFI decision matrix (verified Nov 2026).
 */
object FwFfiBestPractices {
    val strategies: List<FfiStrategy> = listOf(
        FfiStrategy(
            name = "JDK 22+/25 Panama FFM",
            direction = "Bidirectional (downcallHandle + upcallStub)",
            targets = listOf("JVM Desktop", "JVM Server", "Gradle Plugins", "fw-rust-ffi"),
            crateOrApi = "java.lang.foreign.* + Rust extern \"C\"",
            version = "JDK 25 LTS",
            zeroCopy = true,
            whenToUse = "Default for Kotlin/JVM <-> Rust Fat-LTO C ABI libraries (`fw_abi.dll`). Zero JNI boilerplate, Arena-scoped safety, direct upcall callbacks.",
        ),
        FfiStrategy(
            name = "Mozilla UniFFI",
            direction = "Bidirectional (procmacro/UDL + Trait Callbacks + async)",
            targets = listOf("Android (API 21+)", "Kotlin Multiplatform (Gobley)", "JVM", "iOS"),
            crateOrApi = "uniffi",
            version = "0.32.2",
            zeroCopy = false,
            whenToUse = "Default for Android apps & KMP shared libraries needing idiomatic Kotlin data classes, sealed errors, coroutines, and Rust->Kotlin callback interfaces.",
        ),
        FfiStrategy(
            name = "Direct JNI (jni-rs)",
            direction = "Bidirectional (JNIEnv::call_method + native extern)",
            targets = listOf("Android NDK", "JVM"),
            crateOrApi = "jni",
            version = "0.22.4",
            zeroCopy = true,
            whenToUse = "Hot-path Android NDK DirectByteBuffer / AHardwareBuffer / WGPU Surface ANativeWindow interop where zero-copy direct buffers matter.",
        ),
        FfiStrategy(
            name = "Kotlin/Native cinterop",
            direction = "Bidirectional (C header + staticCFunction)",
            targets = listOf("iOS", "macOS", "Linux Native", "Windows Native"),
            crateOrApi = "cbindgen + cinterop (.def)",
            version = "Kotlin 2.4.21",
            zeroCopy = true,
            whenToUse = "Direct LLVM-to-LLVM static linking between Rust staticlib (`libfw_abi.a`) and Kotlin/Native without JVM overhead.",
        ),
    )
}
