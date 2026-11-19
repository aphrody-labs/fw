package dev.aphrody.fw.material3

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Slider
import androidx.compose.material3.SliderState
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import kotlin.math.PI
import kotlin.math.sin

val LocalFwMotionScheme = staticCompositionLocalOf { FwExpressiveTokens.ExpressiveMotion }
val LocalFwRippleFocusRing = staticCompositionLocalOf { FwExpressiveTokens.RippleFocusRing }

/**
 * Unified Material 3 1.5.0 Expressive theme for `fw` and all `aphrody-labs` Compose apps.
 */
@Composable
fun FwExpressiveTheme(
    darkTheme: Boolean = true,
    brandFontFamily: FontFamily = FontFamily.SansSerif,
    motionScheme: FwMotionScheme = FwExpressiveTokens.ExpressiveMotion,
    rippleFocusRing: FwRippleFocusRingConfig = FwExpressiveTokens.RippleFocusRing,
    content: @Composable () -> Unit,
) {
    val colorScheme = if (darkTheme) FwExpressiveTokens.DarkColorScheme else FwExpressiveTokens.LightColorScheme
    CompositionLocalProvider(
        LocalFwMotionScheme provides motionScheme,
        LocalFwRippleFocusRing provides rippleFocusRing,
    ) {
        MaterialTheme(
            colorScheme = colorScheme,
            typography = FwExpressiveTokens.expressiveTypography(brandFontFamily),
            shapes = FwExpressiveTokens.ExpressiveShapes,
            content = content,
        )
    }
}

/**
 * Expressive SplitButton pattern (primary action + trailing menu trigger)
 * aligned with Compose Material 3 1.5.0 `SplitButton`.
 */
@Composable
fun FwSplitActionRow(
    primaryLabel: String,
    onPrimaryClick: () -> Unit,
    secondaryLabel: String = "More",
    onSecondaryClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Row(
        modifier = modifier,
        horizontalArrangement = Arrangement.spacedBy(2.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Button(
            onClick = onPrimaryClick,
            shape = RoundedCornerShape(topStart = 20.dp, bottomStart = 20.dp, topEnd = 6.dp, bottomEnd = 6.dp),
        ) {
            Text(primaryLabel)
        }
        FilledTonalButton(
            onClick = onSecondaryClick,
            shape = RoundedCornerShape(topStart = 6.dp, bottomStart = 6.dp, topEnd = 20.dp, bottomEnd = 20.dp),
        ) {
            Text(secondaryLabel)
        }
    }
}

/**
 * Expressive Wavy Linear Progress Indicator aligned with Material 3 1.5.0
 * `WavyLinearProgressIndicator`.
 */
@Composable
fun FwWavyProgressBar(
    progress: Float,
    phaseRadians: Float = 0f,
    amplitudeDp: Float = 3f,
    wavelengthDp: Float = 24f,
    modifier: Modifier = Modifier.fillMaxWidth().height(12.dp),
) {
    val activeColor = MaterialTheme.colorScheme.primary
    val trackColor = MaterialTheme.colorScheme.surfaceVariant
    Canvas(modifier = modifier) {
        val clamped = progress.coerceIn(0f, 1f)
        val centerY = size.height / 2f
        val activeWidth = size.width * clamped
        val ampPx = amplitudeDp.dp.toPx()
        val wavePx = wavelengthDp.dp.toPx().coerceAtLeast(4f)

        // Inactive straight track
        if (activeWidth < size.width) {
            drawLine(
                color = trackColor,
                start = Offset(activeWidth, centerY),
                end = Offset(size.width, centerY),
                strokeWidth = 4.dp.toPx(),
                cap = StrokeCap.Round,
            )
        }

        // Active expressive sine wave
        if (activeWidth > 1f) {
            val path = Path()
            val step = 2f
            var x = 0f
            while (x <= activeWidth) {
                val y = centerY + sin(((x / wavePx) * 2.0 * PI) + phaseRadians).toFloat() * ampPx
                if (x == 0f) path.moveTo(x, y) else path.lineTo(x, y)
                x += step
            }
            drawPath(
                path = path,
                color = activeColor,
                style = Stroke(width = 4.dp.toPx(), cap = StrokeCap.Round),
            )
        }
    }
}

/**
 * Expressive Floating Toolbar container aligned with Material 3 1.5.0 `HorizontalFloatingToolbar`.
 */
@Composable
fun FwFloatingToolbar(
    modifier: Modifier = Modifier,
    content: @Composable () -> Unit,
) {
    Surface(
        modifier = modifier,
        shape = MaterialTheme.shapes.extraLarge,
        color = MaterialTheme.colorScheme.primaryContainer,
        tonalElevation = 6.dp,
        shadowElevation = 6.dp,
    ) {
        Row(
            modifier = Modifier.padding(horizontal = 12.dp, vertical = 8.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            content()
        }
    }
}

/**
 * Segmented ListItem card aligned with Material 3 1.5.0 segmented `ListItem`.
 */
@Composable
fun FwSegmentedListCard(
    title: String,
    supportingText: String,
    index: Int,
    totalCount: Int,
    modifier: Modifier = Modifier,
) {
    val top = if (index == 0) 20.dp else 6.dp
    val bottom = if (index == totalCount - 1) 20.dp else 6.dp
    Card(
        modifier = modifier.fillMaxWidth(),
        shape = RoundedCornerShape(topStart = top, topEnd = top, bottomStart = bottom, bottomEnd = bottom),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceVariant),
    ) {
        Column(modifier = Modifier.padding(horizontal = 16.dp, vertical = 12.dp)) {
            Text(title, style = MaterialTheme.typography.titleMedium, color = MaterialTheme.colorScheme.onSurface)
            Text(supportingText, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
}

/**
 * Stateful slider row using the Compose Material 3 1.5.0 `SliderState` API directly.
 */
@Composable
fun FwStatefulSliderRow(
    label: String,
    state: SliderState,
    onValueChange: (Float) -> Unit = {},
    modifier: Modifier = Modifier,
) {
    Column(modifier = modifier.fillMaxWidth()) {
        Text("$label: ${"%.2f".format(state.value)}", style = MaterialTheme.typography.labelLarge)
        Slider(
            state = state,
            onValueChange = onValueChange,
        )
    }
}
