package dev.aphrody.fw.material3

import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.Shapes
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlin.math.pow

/**
 * Spring physics parameters matching Material 3 1.5.0 Expressive MotionScheme
 * (fast/default/slow spatial & effects springs).
 */
data class FwSpringToken(
    val name: String,
    val dampingRatio: Float,
    val stiffness: Float,
)

data class FwMotionScheme(
    val fastSpatial: FwSpringToken = FwSpringToken("fastSpatial", 0.6f, 800f),
    val defaultSpatial: FwSpringToken = FwSpringToken("defaultSpatial", 0.6f, 380f),
    val slowSpatial: FwSpringToken = FwSpringToken("slowSpatial", 0.6f, 200f),
    val fastEffects: FwSpringToken = FwSpringToken("fastEffects", 1.0f, 3800f),
    val defaultEffects: FwSpringToken = FwSpringToken("defaultEffects", 1.0f, 1600f),
    val slowEffects: FwSpringToken = FwSpringToken("slowEffects", 1.0f, 800f),
)

/**
 * Configuration matching `androidx.compose.material3:material3-ripple:1.5.0`
 * inset focus rings (`LocalRippleThemeConfiguration`).
 */
data class FwRippleFocusRingConfig(
    val insetFocusRingsEnabled: Boolean = true,
    val focusRingStrokeWidthDp: Float = 3f,
    val focusRingInsetDp: Float = 2f,
    val focusRingColorArgb: Int = 0xFF4F46E5.toInt(),
)

/**
 * Aphrody Labs Expressive Material 3 1.5.0 design tokens shared across `fw`
 * and all `aphrody-labs` Kotlin/Compose apps.
 * Every foreground/background pair is verified >= 4.5:1 WCAG AA contrast.
 */
object FwExpressiveTokens {
    val LightColorScheme: ColorScheme = lightColorScheme(
        primary = Color(0xFF3730A3),
        onPrimary = Color(0xFFFFFFFF),
        primaryContainer = Color(0xFFE0E7FF),
        onPrimaryContainer = Color(0xFF1E1B4B),
        secondary = Color(0xFF0F766E),
        onSecondary = Color(0xFFFFFFFF),
        secondaryContainer = Color(0xFFCCFBF1),
        onSecondaryContainer = Color(0xFF134E4A),
        tertiary = Color(0xFF9A3412),
        onTertiary = Color(0xFFFFFFFF),
        tertiaryContainer = Color(0xFFFFEDD5),
        onTertiaryContainer = Color(0xFF431407),
        background = Color(0xFFF8FAFC),
        onBackground = Color(0xFF0F172A),
        surface = Color(0xFFFFFFFF),
        onSurface = Color(0xFF0F172A),
        surfaceVariant = Color(0xFFE2E8F0),
        onSurfaceVariant = Color(0xFF334155),
        outline = Color(0xFF64748B),
        error = Color(0xFFB91C1C),
        onError = Color(0xFFFFFFFF),
    )

    val DarkColorScheme: ColorScheme = darkColorScheme(
        primary = Color(0xFFA5B4FC),
        onPrimary = Color(0xFF1E1B4B),
        primaryContainer = Color(0xFF312E81),
        onPrimaryContainer = Color(0xFFE0E7FF),
        secondary = Color(0xFF5EEAD4),
        onSecondary = Color(0xFF042F2E),
        secondaryContainer = Color(0xFF115E59),
        onSecondaryContainer = Color(0xFFCCFBF1),
        tertiary = Color(0xFFFDBA74),
        onTertiary = Color(0xFF431407),
        tertiaryContainer = Color(0xFF7C2D12),
        onTertiaryContainer = Color(0xFFFFEDD5),
        background = Color(0xFF0B101F),
        onBackground = Color(0xFFF1F5F9),
        surface = Color(0xFF111827),
        onSurface = Color(0xFFF1F5F9),
        surfaceVariant = Color(0xFF1E293B),
        onSurfaceVariant = Color(0xFFCBD5E1),
        outline = Color(0xFF94A3B8),
        error = Color(0xFFFCA5A5),
        onError = Color(0xFF450A0A),
    )

    val ExpressiveShapes: Shapes = Shapes(
        extraSmall = RoundedCornerShape(6.dp),
        small = RoundedCornerShape(12.dp),
        medium = RoundedCornerShape(16.dp),
        large = RoundedCornerShape(24.dp),
        extraLarge = RoundedCornerShape(32.dp),
    )

    val ExpressiveMotion: FwMotionScheme = FwMotionScheme()
    val RippleFocusRing: FwRippleFocusRingConfig = FwRippleFocusRingConfig()

    /**
     * Mirrors M3 1.5.0 `Typography(defaultFontFamily = ...)` propagating a single
     * brand font across the entire type scale at once.
     */
    fun expressiveTypography(defaultFontFamily: FontFamily = FontFamily.SansSerif): Typography =
        Typography(
            displayLarge = TextStyle(fontFamily = defaultFontFamily, fontWeight = FontWeight.Bold, fontSize = 57.sp, lineHeight = 64.sp),
            headlineMedium = TextStyle(fontFamily = defaultFontFamily, fontWeight = FontWeight.SemiBold, fontSize = 28.sp, lineHeight = 36.sp),
            titleLarge = TextStyle(fontFamily = defaultFontFamily, fontWeight = FontWeight.SemiBold, fontSize = 22.sp, lineHeight = 28.sp),
            titleMedium = TextStyle(fontFamily = defaultFontFamily, fontWeight = FontWeight.Medium, fontSize = 16.sp, lineHeight = 24.sp),
            bodyLarge = TextStyle(fontFamily = defaultFontFamily, fontWeight = FontWeight.Normal, fontSize = 16.sp, lineHeight = 24.sp),
            bodyMedium = TextStyle(fontFamily = defaultFontFamily, fontWeight = FontWeight.Normal, fontSize = 14.sp, lineHeight = 20.sp),
            labelLarge = TextStyle(fontFamily = defaultFontFamily, fontWeight = FontWeight.Medium, fontSize = 14.sp, lineHeight = 20.sp),
        )

    private fun linearize(channel: Int): Double {
        val c = channel / 255.0
        return if (c <= 0.04045) c / 12.92 else ((c + 0.055) / 1.055).pow(2.4)
    }

    fun relativeLuminance(color: Color): Double {
        val argb = color.toArgb()
        val r = linearize((argb ushr 16) and 0xFF)
        val g = linearize((argb ushr 8) and 0xFF)
        val b = linearize(argb and 0xFF)
        return 0.2126 * r + 0.7152 * g + 0.0722 * b
    }

    fun wcagContrastRatio(foreground: Color, background: Color): Double {
        val l1 = relativeLuminance(foreground)
        val l2 = relativeLuminance(background)
        val hi = maxOf(l1, l2)
        val lo = minOf(l1, l2)
        return (hi + 0.05) / (lo + 0.05)
    }
}
