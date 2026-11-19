package com.aphrody.fw.ffi

import kotlin.math.abs
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNotNull
import kotlin.test.assertTrue

class FwRustFfiTest {
    @Test
    fun bidirectionalKotlinToRustAndRustToKotlinWorksLive() {
        val libPath = FwNativeBridge.resolveDefaultLibraryPath()
        assertNotNull(libPath, "fw_abi native library must be built before running FFI integration tests")

        FwNativeBridge(libPath).use { bridge ->
            // 1. Kotlin -> Rust: ABI version & health JSON
            assertEquals(4, bridge.abiVersion())
            val health = bridge.healthJson()
            assertTrue(health.contains("\"kotlin\""), "Expected kotlin grammar in health JSON: $health")
            assertTrue(health.contains("kotlin-ffi-callbacks"), "Expected kotlin-ffi-callbacks capability: $health")

            // 2. Kotlin -> Rust: Tree-sitter Kotlin AST parsing & symbol extraction
            val sampleKt = """
                package com.aphrody.demo
                object Material3Palette {
                    val version: String = "1.5.0"
                }
                class ExpressiveSurface {
                    fun renderWgpu(): Int = 1
                }
            """.trimIndent()
            val summaryJson = bridge.parseAstSummary("kotlin", sampleKt)
            assertTrue(summaryJson.contains("\"ok\":true"), "summary=$summaryJson")
            assertTrue(summaryJson.contains("\"hasError\":false"), "summary=$summaryJson")

            val symbolsJson = bridge.extractAstSymbols("kt", sampleKt)
            assertTrue(symbolsJson.contains("Material3Palette"), "symbols=$symbolsJson")
            assertTrue(symbolsJson.contains("ExpressiveSurface"), "symbols=$symbolsJson")
            assertTrue(symbolsJson.contains("renderWgpu"), "symbols=$symbolsJson")

            // 3. Kotlin -> Rust: GGML dot product & Material 3 WCAG contrast
            val dot = bridge.dotProductF32(floatArrayOf(1f, 2f, 3f), floatArrayOf(4f, 5f, 6f))
            assertTrue(abs(dot - 32f) < 1e-4f, "dot=$dot")

            val bw = bridge.m3ContrastRatio(0xFF000000.toInt(), 0xFFFFFFFF.toInt())
            assertTrue(abs(bw - 21.0) < 1e-2, "bw=$bw")

            // 4. Bidirectional Kotlin -> Rust -> Kotlin upcall callback!
            var receivedFromRust = ""
            val rc = bridge.invokeWithRustCallback("aphrody-labs-m3-1.5.0") { blake3Hex ->
                receivedFromRust = blake3Hex
                blake3Hex.length
            }
            assertEquals(64, rc)
            assertEquals(64, receivedFromRust.length)
            assertTrue(receivedFromRust.all { it in '0'..'9' || it in 'a'..'f' })
        }
    }

    @Test
    fun ffiDecisionMatrixCoversAllVerifiedStrategies() {
        val names = FwFfiBestPractices.strategies.map { it.name }
        assertTrue(names.any { it.contains("Panama") })
        assertTrue(names.any { it.contains("UniFFI") })
        assertTrue(names.any { it.contains("JNI") })
        assertTrue(names.any { it.contains("cinterop") })
    }
}
