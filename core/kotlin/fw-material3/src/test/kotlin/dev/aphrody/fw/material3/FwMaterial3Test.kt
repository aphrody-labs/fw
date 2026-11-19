package dev.aphrody.fw.material3

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class FwMaterial3Test {
    @Test
    fun catalogMatchesOfficialComposeMaterial3_1_5_0Release() {
        val snap = M3Catalog.snapshot
        assertEquals("https://m3.material.io/blog/compose-material-3-is-stable", snap.sourceUrl)
        assertEquals("2026-11-18", snap.datePublished)
        assertEquals("1.5.0", snap.material3Version)
        assertEquals("1.13.0", snap.composeVersion)
        assertEquals("2026.11.01", snap.composeBom)
        assertEquals(35, snap.materialShapes35.size)
        assertEquals(52, snap.experimentalReductionPercent)
        assertTrue(snap.material2Deprecated)
        assertTrue(snap.artifacts.any { it.artifact == "material3-ripple" && it.version == "1.5.0" })
        assertTrue(snap.stableExpressiveComponents.contains("SplitButton"))
        assertTrue(snap.stableExpressiveComponents.contains("WavyLinearProgressIndicator"))
        assertTrue(M3Catalog.toJson().contains("2026.11.01"))
    }

    @Test
    fun expressiveLightAndDarkSchemesMeetWcagAA() {
        val light = FwExpressiveTokens.LightColorScheme
        val dark = FwExpressiveTokens.DarkColorScheme
        val pairs = listOf(
            "light.primary" to (light.onPrimary to light.primary),
            "light.primaryContainer" to (light.onPrimaryContainer to light.primaryContainer),
            "light.secondary" to (light.onSecondary to light.secondary),
            "light.surface" to (light.onSurface to light.surface),
            "light.surfaceVariant" to (light.onSurfaceVariant to light.surfaceVariant),
            "light.error" to (light.onError to light.error),
            "dark.primary" to (dark.onPrimary to dark.primary),
            "dark.primaryContainer" to (dark.onPrimaryContainer to dark.primaryContainer),
            "dark.secondary" to (dark.onSecondary to dark.secondary),
            "dark.surface" to (dark.onSurface to dark.surface),
            "dark.surfaceVariant" to (dark.onSurfaceVariant to dark.surfaceVariant),
            "dark.error" to (dark.onError to dark.error),
        )
        for ((name, pair) in pairs) {
            val ratio = FwExpressiveTokens.wcagContrastRatio(pair.first, pair.second)
            assertTrue(ratio >= 4.5, "Contrast $name was $ratio (< 4.5)")
        }
    }
}
