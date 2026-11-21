plugins {
    alias(libs.plugins.android.library)
    alias(libs.plugins.kotlin.serialization)
    alias(libs.plugins.kotlin.compose)
    `maven-publish`
}

// Android library on androidx Compose Material 3 (stable 1.5.0 through the compose-bom): the only target on which material3 1.5.0
// exists. The Compose Multiplatform port of material3 has no stable release based on 1.5.0 (1.13.0-rc01 is based on 1.5.0-rc01).
android {
    namespace = "${project.group}.material3"
    compileSdk { version = release(37) { minorApiLevel = 1 } }
    defaultConfig { minSdk = 23 }
    publishing { singleVariant("release") { withSourcesJar() } }
}

kotlin {
    compilerOptions {
        allWarningsAsErrors.set(true)
    }
}

dependencies {
    api(project(":fw-core"))
    // Colours, motion scheme and AphrodyTheme: generated from the ay brand tokens, never redefined here.
    api(gradle.extra["fw.theme"] as String)
    api(platform(libs.compose.bom))
    api(libs.compose.material3)
    api(libs.compose.foundation)

    testImplementation(kotlin("test-junit"))
    testImplementation(libs.junit4)
}

publishing {
    publications {
        register<MavenPublication>("release") {
            artifactId = "fw-material3"
            afterEvaluate { from(components["release"]) }
            pom {
                name.set("Fw Compose Material 3 Library")
                description.set("fw components and the Material 3 1.5.0 release catalog on the Aphrody Compose theme (com.aphrody:aphrody-theme).")
                url.set("https://github.com/aphrody-labs/fw")
            }
        }
    }
}
