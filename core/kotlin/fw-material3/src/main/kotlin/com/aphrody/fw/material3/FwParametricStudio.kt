package com.aphrody.fw.material3

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.SliderState
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import kotlin.random.Random

@Serializable
data class ParamRange(
    val name: String,
    val min: Float,
    val max: Float,
    val sweetSpotMin: Float,
    val sweetSpotMax: Float,
    val unit: String = "dp",
) {
    fun clamp(value: Float): Float = value.coerceIn(min, max)
    fun isInSweetSpot(value: Float): Boolean = value in sweetSpotMin..sweetSpotMax
}

@Serializable
data class FwSpecOverlay(
    val showKeylines: Boolean = false,
    val showColorTokens: Boolean = false,
    val showSpatialMeasurements: Boolean = false,
)

/**
 * Decoupled parametric design variables modeled on Material Design's
 * Parametric Prototyping workflow (https://m3.material.io/blog/enhance-your-component-designs-with-parametric-prototyping,
 * published 2026-11-16).
 */
@Serializable
data class FwComponentParams(
    val outerCornerRadiusDp: Float = 20f,
    val innerCornerRadiusDp: Float = 8f,
    val paddingDp: Float = 12f,
    val typeScaleMultiplier: Float = 1.0f,
    val elevationDp: Float = 4f,
    val springDamping: Float = 0.6f,
    val springStiffness: Float = 380f,
    val waveAmplitudeDp: Float = 3f,
    val waveWavelengthDp: Float = 24f,
    val focusRingInsetDp: Float = 2f,
    val specOverlay: FwSpecOverlay = FwSpecOverlay(),
)

@Serializable
data class FwParametricPreset(
    val id: String,
    val label: String,
    val component: String,
    val params: FwComponentParams,
)

@Serializable
data class SweetSpotCell(
    val xValue: Float,
    val yValue: Float,
    val inSweetSpot: Boolean,
)

/**
 * Implements all 6 Material 3 Parametric Prototyping practices:
 * 1. Map sweet spots (bounded grid sweep)
 * 2. Fine-tune decoupled variables
 * 3. Group / tether dependent parameters via constraint rules
 * 4. Spec overlays (keylines, color tokens, spatial measurements)
 * 5. Save/load presets + JSON export
 * 6. Deterministic randomization to break bias and stress-test edge cases
 */
object FwParametricEngine {
    val CornerRadiusRange = ParamRange("outerCornerRadiusDp", 0f, 36f, 12f, 28f, "dp")
    val PaddingRange = ParamRange("paddingDp", 4f, 32f, 8f, 20f, "dp")
    val TypeScaleRange = ParamRange("typeScaleMultiplier", 0.85f, 1.4f, 0.95f, 1.15f, "x")
    val SpringDampingRange = ParamRange("springDamping", 0.35f, 1.0f, 0.55f, 0.85f, "ratio")
    val SpringStiffnessRange = ParamRange("springStiffness", 150f, 1200f, 200f, 800f, "N/m")
    val WaveAmplitudeRange = ParamRange("waveAmplitudeDp", 0f, 8f, 2f, 4.5f, "dp")

    private val prettyJson = Json { prettyPrint = true; encodeDefaults = true; ignoreUnknownKeys = true }

    val builtInPresets: List<FwParametricPreset> = listOf(
        FwParametricPreset("expressive-time-picker", "Expressive Time Picker", "VibrantTimePickerDialog", FwComponentParams()),
        FwParametricPreset(
            "compact-toolbar",
            "Compact Floating Toolbar",
            "HorizontalFloatingToolbar",
            FwComponentParams(outerCornerRadiusDp = 28f, innerCornerRadiusDp = 16f, paddingDp = 10f, elevationDp = 6f),
        ),
        FwParametricPreset(
            "high-contrast-spec",
            "Spec Inspection Mode",
            "ScrollField",
            FwComponentParams(
                outerCornerRadiusDp = 16f,
                innerCornerRadiusDp = 6f,
                paddingDp = 12f,
                specOverlay = FwSpecOverlay(showKeylines = true, showColorTokens = true, showSpatialMeasurements = true),
            ),
        ),
    )

    /** Rule 3: Tether dependent parameters so inner radius never exceeds outer radius minus padding. */
    fun enforceConstraints(raw: FwComponentParams): FwComponentParams {
        val outer = CornerRadiusRange.clamp(raw.outerCornerRadiusDp)
        val pad = PaddingRange.clamp(raw.paddingDp)
        val maxInner = (outer - pad * 0.5f).coerceAtLeast(2f)
        val inner = raw.innerCornerRadiusDp.coerceIn(2f, maxInner)
        return raw.copy(
            outerCornerRadiusDp = outer,
            innerCornerRadiusDp = inner,
            paddingDp = pad,
            typeScaleMultiplier = TypeScaleRange.clamp(raw.typeScaleMultiplier),
            elevationDp = raw.elevationDp.coerceIn(0f, 16f),
            springDamping = SpringDampingRange.clamp(raw.springDamping),
            springStiffness = SpringStiffnessRange.clamp(raw.springStiffness),
            waveAmplitudeDp = WaveAmplitudeRange.clamp(raw.waveAmplitudeDp),
            waveWavelengthDp = raw.waveWavelengthDp.coerceIn(12f, 48f),
            focusRingInsetDp = raw.focusRingInsetDp.coerceIn(1f, 6f),
        )
    }

    /** Rule 1: Generate an organized grid of combinations marking sweet spots vs off-limits zones. */
    fun mapSweetSpotGrid(rangeX: ParamRange, rangeY: ParamRange, steps: Int = 5): List<SweetSpotCell> {
        require(steps >= 2) { "steps must be >= 2" }
        val cells = ArrayList<SweetSpotCell>(steps * steps)
        for (iy in 0 until steps) {
            val fy = iy.toFloat() / (steps - 1)
            val y = rangeY.min + (rangeY.max - rangeY.min) * fy
            for (ix in 0 until steps) {
                val fx = ix.toFloat() / (steps - 1)
                val x = rangeX.min + (rangeX.max - rangeX.min) * fx
                cells.add(SweetSpotCell(x, y, rangeX.isInSweetSpot(x) && rangeY.isInSweetSpot(y)))
            }
        }
        return cells
    }

    /** Rule 6: Seeded randomization within sweet spots or full range for stress testing. */
    fun randomize(seed: Long, stayInSweetSpot: Boolean = false): FwComponentParams {
        val rng = Random(seed)
        fun sample(r: ParamRange): Float {
            val lo = if (stayInSweetSpot) r.sweetSpotMin else r.min
            val hi = if (stayInSweetSpot) r.sweetSpotMax else r.max
            return lo + rng.nextFloat() * (hi - lo)
        }
        val raw = FwComponentParams(
            outerCornerRadiusDp = sample(CornerRadiusRange),
            innerCornerRadiusDp = sample(CornerRadiusRange) * 0.5f,
            paddingDp = sample(PaddingRange),
            typeScaleMultiplier = sample(TypeScaleRange),
            springDamping = sample(SpringDampingRange),
            springStiffness = sample(SpringStiffnessRange),
            waveAmplitudeDp = sample(WaveAmplitudeRange),
        )
        return enforceConstraints(raw)
    }

    /** Rule 5: Export / import presets as JSON for AI prompts and design-engineering handoff. */
    fun exportPresetJson(preset: FwParametricPreset): String = prettyJson.encodeToString(preset)
    fun importPresetJson(json: String): FwParametricPreset = prettyJson.decodeFromString(json)
}

/**
 * Interactive Compose Material 3 1.5.0 parametric control panel (sliders, switches, preset/randomize actions).
 */
@Composable
fun FwParametricControlPanel(
    params: FwComponentParams,
    onParamsChange: (FwComponentParams) -> Unit,
    onRandomize: () -> Unit,
    onExportJson: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val cornerNorm = ((params.outerCornerRadiusDp - FwParametricEngine.CornerRadiusRange.min) /
        (FwParametricEngine.CornerRadiusRange.max - FwParametricEngine.CornerRadiusRange.min)).coerceIn(0f, 1f)
    val paddingNorm = ((params.paddingDp - FwParametricEngine.PaddingRange.min) /
        (FwParametricEngine.PaddingRange.max - FwParametricEngine.PaddingRange.min)).coerceIn(0f, 1f)
    val cornerState = remember(cornerNorm) { SliderState(value = cornerNorm) }
    val paddingState = remember(paddingNorm) { SliderState(value = paddingNorm) }

    Column(
        modifier = modifier.fillMaxWidth().padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text("Parametric Prototype Controls", style = MaterialTheme.typography.titleLarge)
        FwStatefulSliderRow(
            label = "Outer Corner Radius (${params.outerCornerRadiusDp.toInt()} dp)",
            state = cornerState,
            onValueChange = { norm ->
                val dp = FwParametricEngine.CornerRadiusRange.min +
                    norm * (FwParametricEngine.CornerRadiusRange.max - FwParametricEngine.CornerRadiusRange.min)
                onParamsChange(FwParametricEngine.enforceConstraints(params.copy(outerCornerRadiusDp = dp)))
            },
        )
        FwStatefulSliderRow(
            label = "Padding (${params.paddingDp.toInt()} dp)",
            state = paddingState,
            onValueChange = { norm ->
                val dp = FwParametricEngine.PaddingRange.min +
                    norm * (FwParametricEngine.PaddingRange.max - FwParametricEngine.PaddingRange.min)
                onParamsChange(FwParametricEngine.enforceConstraints(params.copy(paddingDp = dp)))
            },
        )
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text("Keylines & Spec Overlay", style = MaterialTheme.typography.bodyMedium)
            Switch(
                checked = params.specOverlay.showKeylines,
                onCheckedChange = { on ->
                    onParamsChange(
                        params.copy(
                            specOverlay = params.specOverlay.copy(
                                showKeylines = on,
                                showColorTokens = on,
                                showSpatialMeasurements = on,
                            ),
                        ),
                    )
                },
            )
        }
        FwSplitActionRow(
            primaryLabel = "Randomize",
            onPrimaryClick = onRandomize,
            secondaryLabel = "Export JSON",
            onSecondaryClick = onExportJson,
        )
    }
}
