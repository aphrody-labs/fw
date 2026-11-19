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

    @Test
    fun parametricPrototypingCoversAllSixMaterial3Pillars() {
        // 1. Sweet spot grid
        val grid = FwParametricEngine.mapSweetSpotGrid(
            FwParametricEngine.CornerRadiusRange,
            FwParametricEngine.PaddingRange,
            steps = 5,
        )
        assertEquals(25, grid.size)
        assertTrue(grid.any { it.inSweetSpot })
        assertTrue(grid.any { !it.inSweetSpot })

        // 2 & 3. Constraint tethering prevents inner radius > outer radius - padding*0.5
        val tethered = FwParametricEngine.enforceConstraints(
            FwComponentParams(outerCornerRadiusDp = 10f, innerCornerRadiusDp = 30f, paddingDp = 12f),
        )
        assertTrue(tethered.innerCornerRadiusDp <= tethered.outerCornerRadiusDp - tethered.paddingDp * 0.5f + 0.01f)

        // 4 & 5. Spec overlay & JSON preset round-trip
        val preset = FwParametricEngine.builtInPresets.last()
        assertTrue(preset.params.specOverlay.showKeylines)
        val roundTripped = FwParametricEngine.importPresetJson(FwParametricEngine.exportPresetJson(preset))
        assertEquals(preset, roundTripped)

        // 6. Seeded randomization is deterministic and stays in sweet spot when requested
        val r1 = FwParametricEngine.randomize(seed = 42L, stayInSweetSpot = true)
        val r2 = FwParametricEngine.randomize(seed = 42L, stayInSweetSpot = true)
        assertEquals(r1, r2)
        assertTrue(FwParametricEngine.CornerRadiusRange.isInSweetSpot(r1.outerCornerRadiusDp))
        assertTrue(FwParametricEngine.PaddingRange.isInSweetSpot(r1.paddingDp))
    }
}
