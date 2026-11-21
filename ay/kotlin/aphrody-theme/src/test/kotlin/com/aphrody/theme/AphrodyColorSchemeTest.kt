package com.aphrody.theme

import androidx.compose.material3.ColorScheme
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import java.io.File
import kotlin.math.pow
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNotEquals
import kotlin.test.assertTrue
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

class AphrodyColorSchemeTest {
    private val tokens: JsonObject =
        Json.parseToJsonElement(File(requireNotNull(System.getProperty("aphrody.tokens"))).readText()).jsonObject

    private val roles: List<Pair<String, (ColorScheme) -> Color>> = listOf(
        "primary" to { s -> s.primary }, "onPrimary" to { s -> s.onPrimary },
        "primaryContainer" to { s -> s.primaryContainer }, "onPrimaryContainer" to { s -> s.onPrimaryContainer },
        "inversePrimary" to { s -> s.inversePrimary },
        "secondary" to { s -> s.secondary }, "onSecondary" to { s -> s.onSecondary },
        "secondaryContainer" to { s -> s.secondaryContainer }, "onSecondaryContainer" to { s -> s.onSecondaryContainer },
        "tertiary" to { s -> s.tertiary }, "onTertiary" to { s -> s.onTertiary },
        "tertiaryContainer" to { s -> s.tertiaryContainer }, "onTertiaryContainer" to { s -> s.onTertiaryContainer },
        "background" to { s -> s.background }, "onBackground" to { s -> s.onBackground },
        "surface" to { s -> s.surface }, "onSurface" to { s -> s.onSurface },
        "surfaceVariant" to { s -> s.surfaceVariant }, "onSurfaceVariant" to { s -> s.onSurfaceVariant },
        "surfaceTint" to { s -> s.surfaceTint },
        "inverseSurface" to { s -> s.inverseSurface }, "inverseOnSurface" to { s -> s.inverseOnSurface },
        "error" to { s -> s.error }, "onError" to { s -> s.onError },
        "errorContainer" to { s -> s.errorContainer }, "onErrorContainer" to { s -> s.onErrorContainer },
        "outline" to { s -> s.outline }, "outlineVariant" to { s -> s.outlineVariant }, "scrim" to { s -> s.scrim },
        "surfaceBright" to { s -> s.surfaceBright }, "surfaceContainer" to { s -> s.surfaceContainer },
        "surfaceContainerHigh" to { s -> s.surfaceContainerHigh },
        "surfaceContainerHighest" to { s -> s.surfaceContainerHighest },
        "surfaceContainerLow" to { s -> s.surfaceContainerLow },
        "surfaceContainerLowest" to { s -> s.surfaceContainerLowest }, "surfaceDim" to { s -> s.surfaceDim },
        "primaryFixed" to { s -> s.primaryFixed }, "primaryFixedDim" to { s -> s.primaryFixedDim },
        "onPrimaryFixed" to { s -> s.onPrimaryFixed }, "onPrimaryFixedVariant" to { s -> s.onPrimaryFixedVariant },
        "secondaryFixed" to { s -> s.secondaryFixed }, "secondaryFixedDim" to { s -> s.secondaryFixedDim },
        "onSecondaryFixed" to { s -> s.onSecondaryFixed }, "onSecondaryFixedVariant" to { s -> s.onSecondaryFixedVariant },
        "tertiaryFixed" to { s -> s.tertiaryFixed }, "tertiaryFixedDim" to { s -> s.tertiaryFixedDim },
        "onTertiaryFixed" to { s -> s.onTertiaryFixed }, "onTertiaryFixedVariant" to { s -> s.onTertiaryFixedVariant },
    )

    private fun hex(c: Color): String = "#%06x".format(c.toArgb() and 0xFFFFFF)

    @Test
    fun everyRoleEqualsTheTokenInBothSchemes() {
        assertEquals(48, roles.size)
        for ((mode, scheme) in listOf("light" to AphrodyLightColors, "dark" to AphrodyDarkColors)) {
            val expected = tokens.getValue(mode).jsonObject
            for ((name, role) in roles) {
                assertEquals(expected.getValue(name).jsonPrimitive.content.lowercase(), hex(role(scheme)), "$mode.$name")
            }
        }
    }

    @Test
    fun lightAndDarkDiffer() {
        assertNotEquals(AphrodyLightColors.surface, AphrodyDarkColors.surface)
        assertNotEquals(AphrodyLightColors.primary, AphrodyDarkColors.primary)
    }

    private fun luminance(c: Color): Double {
        fun lin(v: Int): Double = (v / 255.0).let { if (it <= 0.04045) it / 12.92 else ((it + 0.055) / 1.055).pow(2.4) }
        val argb = c.toArgb()
        return 0.2126 * lin((argb shr 16) and 0xFF) + 0.7152 * lin((argb shr 8) and 0xFF) + 0.0722 * lin(argb and 0xFF)
    }

    private fun contrast(a: Color, b: Color): Double {
        val (hi, lo) = luminance(a).let { x -> luminance(b).let { y -> maxOf(x, y) to minOf(x, y) } }
        return (hi + 0.05) / (lo + 0.05)
    }

    @Test
    fun contentColoursMeetWcagAaOnTheirContainers() {
        for ((mode, s) in listOf("light" to AphrodyLightColors, "dark" to AphrodyDarkColors)) {
            val pairs = mapOf(
                "primary" to (s.onPrimary to s.primary),
                "primaryContainer" to (s.onPrimaryContainer to s.primaryContainer),
                "secondary" to (s.onSecondary to s.secondary),
                "secondaryContainer" to (s.onSecondaryContainer to s.secondaryContainer),
                "tertiary" to (s.onTertiary to s.tertiary),
                "tertiaryContainer" to (s.onTertiaryContainer to s.tertiaryContainer),
                "error" to (s.onError to s.error),
                "errorContainer" to (s.onErrorContainer to s.errorContainer),
                "background" to (s.onBackground to s.background),
                "surface" to (s.onSurface to s.surface),
                "surfaceVariant" to (s.onSurfaceVariant to s.surfaceVariant),
                "inverseSurface" to (s.inverseOnSurface to s.inverseSurface),
            )
            for ((name, pair) in pairs) {
                val ratio = contrast(pair.first, pair.second)
                assertTrue(ratio >= 4.5, "$mode.$name contrast $ratio < 4.5")
            }
        }
    }

    @Test
    fun packageIsTheReversedRootDomain() {
        assertEquals("AphrodyColorSchemeKt", Class.forName(requireNotNull(System.getProperty("aphrody.theme.package")) + ".AphrodyColorSchemeKt").simpleName)
    }
}
