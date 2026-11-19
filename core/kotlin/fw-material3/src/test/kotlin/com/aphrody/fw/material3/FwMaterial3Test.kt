package com.aphrody.fw.material3

import com.aphrody.theme.AphrodyDarkColors
import com.aphrody.theme.AphrodyLightColors
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNotEquals
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
    fun colourSchemesComeFromTheAyThemeArtifactNotFromFw() {
        assertNotEquals(AphrodyLightColors, AphrodyDarkColors)
        assertEquals("com.aphrody.theme", Class.forName("com.aphrody.theme.AphrodyColorSchemeKt").packageName)
        assertTrue(
            runCatching { Class.forName("com.aphrody.fw.material3.FwExpressiveTokens") }.isFailure,
            "fw must not define a palette of its own",
        )
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
