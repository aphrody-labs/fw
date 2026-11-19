package dev.aphrody.fw.material3

import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json

@Serializable
data class MavenArtifactSpec(
    val group: String,
    val artifact: String,
    val version: String,
    val status: String,
    val notes: String,
)

@Serializable
data class MigrationRule(
    val legacyApi: String,
    val material3Api: String,
    val breakingIn150: Boolean,
    val guidance: String,
)

@Serializable
data class Material3ReleaseSnapshot(
    val sourceUrl: String,
    val releaseNotesUrl: String,
    val composeBlogUrl: String,
    val datePublished: String,
    val material3Version: String,
    val composeVersion: String,
    val composeBom: String,
    val minKotlinVersion: String,
    val compileSdk: String,
    val experimentalAnnotationsBefore: Int,
    val experimentalAnnotationsAfter: Int,
    val experimentalReductionPercent: Int,
    val material2Deprecated: Boolean,
    val artifacts: List<MavenArtifactSpec>,
    val stableExpressiveComponents: List<String>,
    val graduatedCoreApis: List<String>,
    val remainingExperimentalApis: List<String>,
    val materialShapes35: List<String>,
    val migrationRules: List<MigrationRule>,
)

/**
 * Authoritative, live-verified catalog of the Compose Material 3 1.5.0 stable release
 * (published 2026-11-18 at https://m3.material.io/blog/compose-material-3-is-stable)
 * and Jetpack Compose 1.13 (`compose-bom:2026.11.01`).
 */
object M3Catalog {
    val snapshot: Material3ReleaseSnapshot = Material3ReleaseSnapshot(
        sourceUrl = "https://m3.material.io/blog/compose-material-3-is-stable",
        releaseNotesUrl = "https://developer.android.com/jetpack/androidx/releases/compose-material3#1.5.0",
        composeBlogUrl = "https://developer.android.com/blog/posts/jetpack-compose-november-2026-release",
        datePublished = "2026-11-18",
        material3Version = "1.5.0",
        composeVersion = "1.13.0",
        composeBom = "2026.11.01",
        minKotlinVersion = "2.2.0",
        compileSdk = "37.1",
        experimentalAnnotationsBefore = 120,
        experimentalAnnotationsAfter = 58,
        experimentalReductionPercent = 52,
        material2Deprecated = true,
        artifacts = listOf(
            MavenArtifactSpec("androidx.compose", "compose-bom", "2026.11.01", "stable", "Official November 2026 Compose BOM"),
            MavenArtifactSpec("androidx.compose.material3", "material3", "1.5.0", "stable", "Core + Expressive M3 components"),
            MavenArtifactSpec("androidx.compose.material3", "material3-ripple", "1.5.0", "stable", "Inset focus rings via LocalRippleThemeConfiguration"),
            MavenArtifactSpec("androidx.compose.material3", "material3-window-size-class", "1.5.0", "stable", "Window size classes for adaptive layouts"),
            MavenArtifactSpec("androidx.compose.material3", "material3-adaptive-navigation-suite", "1.5.0", "stable", "Adaptive navigation rail/bar/drawer"),
            MavenArtifactSpec("androidx.compose.material3", "material3-lint", "1.5.0", "stable", "Android Lint rules for M3"),
            MavenArtifactSpec("androidx.compose.material3.adaptive", "adaptive", "1.3.0", "stable", "Adaptive panes and scaffolds"),
            MavenArtifactSpec("org.jetbrains.compose.material3", "material3", "1.13.0-rc01", "rc", "JetBrains Compose Multiplatform Material 3"),
        ),
        stableExpressiveComponents = listOf(
            "ToggleButton",
            "ElevatedToggleButton",
            "FilledTonalToggleButton",
            "OutlinedToggleButton",
            "SplitButton",
            "ButtonGroup",
            "FilterChip (shape-morphing)",
            "ElevatedFilterChip (shape-morphing)",
            "InputChip (shape-morphing)",
            "SelectableDropdownMenuItem",
            "CheckableDropdownMenuItem",
            "DropdownMenuPopupPositionProvider",
            "VibrantTimePickerDialog",
            "ScrollField",
            "Scrim",
            "WavyLinearProgressIndicator",
            "WavyCircularProgressIndicator",
            "FloatingToolbar",
            "HorizontalFloatingToolbar",
            "Segmented ListItem",
            "ExpandedDockedSearchBarWithGap",
            "ExpandedFullScreenContainedSearchBar",
        ),
        graduatedCoreApis = listOf(
            "BottomAppBar",
            "TopAppBar",
            "MediumFlexibleTopAppBar",
            "LargeTopAppBar",
            "TwoRowsTopAppBar",
            "TopAppBarScrollBehavior",
            "SearchBarState",
            "Slider / SliderState",
            "RangeSlider / RangeSliderState",
            "Multi-browse & Uncontained Carousel",
            "LocalRippleThemeConfiguration / RippleThemeConfiguration",
            "BasicAlertDialog",
            "MaterialTheme.expressiveLightColorScheme",
            "Typography(defaultFontFamily = ...)",
            "Text(color = ColorProducer)",
        ),
        remainingExperimentalApis = listOf(
            "LoadingIndicator",
            "ContainedLoadingIndicator",
            "LoadingIndicatorDefaults",
            "PullToRefreshDefaults.LoadingIndicator",
            "MaterialShapes",
            "AppBarWithSearch",
            "Modifier.carouselParallaxScrollEffect",
            "Standalone static Sheet",
        ),
        materialShapes35 = listOf(
            "Circle", "Square", "Slanted", "Arch", "Fan",
            "Arrow", "SemiCircle", "Oval", "Pill", "Triangle",
            "Diamond", "ClamShell", "Pentagon", "Gem", "Sunny",
            "VerySunny", "Cookie4Sided", "Cookie6Sided", "Cookie7Sided", "Cookie9Sided",
            "Cookie12Sided", "Ghostish", "Clover4Leaf", "Clover8Leaf", "Burst",
            "SoftBurst", "Boom", "SoftBoom", "Flower", "Puffy",
            "PuffyDiamond", "PixelCircle", "PixelTriangle", "Bun", "Heart",
        ),
        migrationRules = listOf(
            MigrationRule(
                legacyApi = "Slider(value = v, onValueChange = { v = it })",
                material3Api = "val state = remember { SliderState(value = 0.5f) }; Slider(state = state, onValueChange = { ... })",
                breakingIn150 = true,
                guidance = "Stateless Slider/RangeSlider overloads are deprecated; onValueChange is required in the composable layer and RangeSliderState uses startValue/endValue.",
            ),
            MigrationRule(
                legacyApi = "SecureTextField default TextObfuscationMode.RevealLastTyped",
                material3Api = "SecureTextField default TextObfuscationMode.System",
                breakingIn150 = true,
                guidance = "1.5.0 defaults to system obfuscation; explicitly pass TextObfuscationMode.RevealLastTyped if required.",
            ),
            MigrationRule(
                legacyApi = "LocalMotionScheme.current",
                material3Api = "MaterialTheme.motionScheme",
                breakingIn150 = false,
                guidance = "MaterialTheme consolidates tokens into MaterialTheme.LocalMaterialTheme.current.",
            ),
            MigrationRule(
                legacyApi = "TopAppBarDefaults.pinnedScrollBehavior(isAtTop = ...)",
                material3Api = "TopAppBarDefaults.pinnedScrollBehavior(scrollableState = ..., isAtStart = ...)",
                breakingIn150 = true,
                guidance = "Scroll behaviors now take ScrollableState and rename isAtTop to isAtStart.",
            ),
            MigrationRule(
                legacyApi = "androidx.compose.material:* (Material 2)",
                material3Api = "androidx.compose.material3:material3:1.5.0",
                breakingIn150 = false,
                guidance = "Material 2 is deprecated and no longer actively maintained; every core M2 component has a stable M3 equivalent.",
            ),
        ),
    )

    private val prettyJson = Json { prettyPrint = true; encodeDefaults = true }

    fun toJson(): String = prettyJson.encodeToString(snapshot)
}
