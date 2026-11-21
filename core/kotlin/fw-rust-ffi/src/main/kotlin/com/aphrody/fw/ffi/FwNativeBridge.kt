package com.aphrody.fw.ffi

import java.lang.foreign.Arena
import java.lang.foreign.FunctionDescriptor
import java.lang.foreign.Linker
import java.lang.foreign.MemorySegment
import java.lang.foreign.SymbolLookup
import java.lang.foreign.ValueLayout
import java.lang.invoke.MethodHandle
import java.lang.invoke.MethodHandles
import java.lang.invoke.MethodType
import java.nio.charset.StandardCharsets
import java.nio.file.Files
import java.nio.file.Path

/**
 * Bidirectional Kotlin <-> Rust FFI bridge over JDK 22+/25 Panama Foreign Function & Memory API.
 *
 * - **Kotlin -> Rust**: Downcall `MethodHandle`s into Fat-LTO `fw_abi.dll` / `libfw_abi.so` / `libfw_abi.dylib`
 *   (zero JNI glue, zero JNA reflection overhead, deterministic `Arena` memory lifetime).
 * - **Rust -> Kotlin**: Native C callback function pointer allocated via `Linker.upcallStub`,
 *   allowing Rust `extern "C"` code to invoke Kotlin lambdas directly.
 */
class FwNativeBridge(private val libraryPath: Path) : AutoCloseable {
    private val libraryArena: Arena = Arena.ofShared()
    private val linker: Linker = Linker.nativeLinker()
    private val lookup: SymbolLookup = SymbolLookup.libraryLookup(libraryPath, libraryArena)

    private val hAbiVersion: MethodHandle = downcall(
        "fw_abi_version",
        FunctionDescriptor.of(ValueLayout.JAVA_INT),
    )

    private val hHealthJson: MethodHandle = downcall(
        "fw_health_json",
        FunctionDescriptor.of(ValueLayout.ADDRESS),
    )

    private val hParseSummary: MethodHandle = downcall(
        "fw_parse_summary",
        FunctionDescriptor.of(
            ValueLayout.JAVA_LONG,
            ValueLayout.ADDRESS,
            ValueLayout.JAVA_LONG,
            ValueLayout.ADDRESS,
            ValueLayout.JAVA_LONG,
            ValueLayout.ADDRESS,
            ValueLayout.JAVA_LONG,
        ),
    )

    private val hAstSymbols: MethodHandle = downcall(
        "fw_ast_symbols",
        FunctionDescriptor.of(
            ValueLayout.JAVA_LONG,
            ValueLayout.ADDRESS,
            ValueLayout.JAVA_LONG,
            ValueLayout.ADDRESS,
            ValueLayout.JAVA_LONG,
            ValueLayout.ADDRESS,
            ValueLayout.JAVA_LONG,
        ),
    )

    private val hMlDotF32: MethodHandle = downcall(
        "fw_ml_dot_f32",
        FunctionDescriptor.of(
            ValueLayout.JAVA_FLOAT,
            ValueLayout.ADDRESS,
            ValueLayout.ADDRESS,
            ValueLayout.JAVA_LONG,
        ),
    )

    private val hM3ContrastRatio: MethodHandle = downcall(
        "fw_m3_contrast_ratio",
        FunctionDescriptor.of(
            ValueLayout.JAVA_DOUBLE,
            ValueLayout.JAVA_INT,
            ValueLayout.JAVA_INT,
        ),
    )

    private val hKotlinInvokeCallback: MethodHandle = downcall(
        "fw_kotlin_invoke_callback",
        FunctionDescriptor.of(
            ValueLayout.JAVA_INT,
            ValueLayout.ADDRESS,
            ValueLayout.JAVA_LONG,
            ValueLayout.ADDRESS,
        ),
    )

    private fun downcall(symbol: String, descriptor: FunctionDescriptor): MethodHandle {
        val addr = lookup.find(symbol).orElseThrow {
            UnsatisfiedLinkError("Symbol '$symbol' not found in $libraryPath")
        }
        return linker.downcallHandle(addr, descriptor)
    }

    /** Kotlin -> Rust: returns ABI version (4). */
    fun abiVersion(): Int = hAbiVersion.invokeExact() as Int

    /** Kotlin -> Rust: returns static NUL-terminated health JSON from `fw_abi`. */
    fun healthJson(): String {
        val ptr = hHealthJson.invokeExact() as MemorySegment
        return ptr.reinterpret(16_384).getString(0, StandardCharsets.UTF_8)
    }

    /** Kotlin -> Rust: parses any of 12 Tree-sitter grammars (including `kotlin` via `tree-sitter-kotlin-ng`). */
    fun parseAstSummary(lang: String, source: String, outBufBytes: Long = 16_384L): String =
        invokeAstJson(hParseSummary, lang, source, outBufBytes)

    /** Kotlin -> Rust: extracts AST symbols (classes, objects, functions) using Rust Tree-sitter. */
    fun extractAstSymbols(lang: String, source: String, outBufBytes: Long = 65_536L): String =
        invokeAstJson(hAstSymbols, lang, source, outBufBytes)

    private fun invokeAstJson(
        handle: MethodHandle,
        lang: String,
        source: String,
        outBufBytes: Long,
    ): String = Arena.ofConfined().use { arena ->
        val langBytes = lang.toByteArray(StandardCharsets.UTF_8)
        val srcBytes = source.toByteArray(StandardCharsets.UTF_8)
        val langSeg = arena.allocate(langBytes.size.toLong())
        langSeg.copyFrom(MemorySegment.ofArray(langBytes))
        val srcSeg = arena.allocate(srcBytes.size.toLong())
        srcSeg.copyFrom(MemorySegment.ofArray(srcBytes))
        val outSeg = arena.allocate(outBufBytes)

        val written = handle.invokeExact(
            langSeg,
            langBytes.size.toLong(),
            srcSeg,
            srcBytes.size.toLong(),
            outSeg,
            outBufBytes,
        ) as Long
        if (written <= 0L) return ""
        val bytes = ByteArray(written.toInt())
        MemorySegment.copy(outSeg, ValueLayout.JAVA_BYTE, 0L, bytes, 0, written.toInt())
        String(bytes, StandardCharsets.UTF_8)
    }

    /** Kotlin -> Rust: SIMD/native f32 dot product in Rust GGML kernel. */
    fun dotProductF32(a: FloatArray, b: FloatArray): Float {
        require(a.size == b.size) { "Vector lengths must match: ${a.size} != ${b.size}" }
        if (a.isEmpty()) return 0f
        return Arena.ofConfined().use { arena ->
            val segA = arena.allocateFrom(ValueLayout.JAVA_FLOAT, *a)
            val segB = arena.allocateFrom(ValueLayout.JAVA_FLOAT, *b)
            hMlDotF32.invokeExact(segA, segB, a.size.toLong()) as Float
        }
    }

    /** Kotlin -> Rust: native WCAG 2.1 contrast ratio for Material 3 1.5.0 color verification. */
    fun m3ContrastRatio(fgArgb: Int, bgArgb: Int): Double =
        hM3ContrastRatio.invokeExact(fgArgb, bgArgb) as Double

    /**
     * **Bidirectional Kotlin -> Rust -> Kotlin**:
     * Sends `input` bytes to Rust `fw_kotlin_invoke_callback`, which computes BLAKE3 in native Rust
     * and invokes our Kotlin lambda via a Panama `upcallStub` C function pointer!
     */
    fun invokeWithRustCallback(input: String, onBlake3Hex: (String) -> Int): Int =
        Arena.ofConfined().use { arena ->
            val inputBytes = input.toByteArray(StandardCharsets.UTF_8)
            val inputSeg = arena.allocate(inputBytes.size.toLong().coerceAtLeast(1L))
            if (inputBytes.isNotEmpty()) {
                inputSeg.copyFrom(MemorySegment.ofArray(inputBytes))
            }

            val trampoline = CallbackTrampoline(onBlake3Hex)
            val targetHandle = MethodHandles.lookup().findVirtual(
                CallbackTrampoline::class.java,
                "invoke",
                MethodType.methodType(Int::class.javaPrimitiveType, MemorySegment::class.java, Long::class.javaPrimitiveType),
            ).bindTo(trampoline)

            val upcallDescriptor = FunctionDescriptor.of(
                ValueLayout.JAVA_INT,
                ValueLayout.ADDRESS,
                ValueLayout.JAVA_LONG,
            )
            val upcallStub = linker.upcallStub(targetHandle, upcallDescriptor, arena)
            hKotlinInvokeCallback.invokeExact(inputSeg, inputBytes.size.toLong(), upcallStub) as Int
        }

    class CallbackTrampoline(private val callback: (String) -> Int) {
        fun invoke(ptr: MemorySegment, len: Long): Int {
            if (len <= 0L) return callback("")
            val bytes = ByteArray(len.toInt())
            MemorySegment.copy(ptr.reinterpret(len), ValueLayout.JAVA_BYTE, 0L, bytes, 0, len.toInt())
            return callback(String(bytes, StandardCharsets.UTF_8))
        }
    }

    override fun close() {
        libraryArena.close()
    }

    companion object {
        /**
         * Resolves `fw_abi.dll` / `libfw_abi.so` / `libfw_abi.dylib` from explicit path,
         * `FW_ABI_PATH` env var, or standard `fw` build output locations.
         */
        fun resolveDefaultLibraryPath(repoRoot: Path? = null): Path? {
            val envPath = System.getenv("FW_ABI_PATH")?.let { Path.of(it) }
            if (envPath != null && Files.isRegularFile(envPath)) return envPath

            val libName = when {
                System.getProperty("os.name").lowercase().contains("win") -> "fw_abi.dll"
                System.getProperty("os.name").lowercase().contains("mac") -> "libfw_abi.dylib"
                else -> "libfw_abi.so"
            }
            val roots = buildList {
                if (repoRoot != null) add(repoRoot)
                add(Path.of("").toAbsolutePath())
                add(Path.of("..", "..").toAbsolutePath().normalize())
                add(Path.of("..", "..", "..").toAbsolutePath().normalize())
            }
            val cargoTarget = System.getenv("CARGO_TARGET_DIR")?.let { Path.of(it) }
            val candidates = buildList {
                if (cargoTarget != null) {
                    add(cargoTarget.resolve("release").resolve(libName))
                    add(cargoTarget.resolve("debug").resolve(libName))
                }
                add(Path.of("C:/data/cache/cargo-target/release", libName))
                add(Path.of("C:/data/cache/cargo-target/debug", libName))
                for (r in roots) {
                    add(r.resolve("target/fw").resolve(libName))
                    add(r.resolve("dist").resolve(libName))
                    add(r.resolve("native/fw-abi/target/release").resolve(libName))
                    add(r.resolve("native/fw-abi/target/debug").resolve(libName))
                }
            }
            return candidates.firstOrNull { Files.isRegularFile(it) }
        }
    }
}
