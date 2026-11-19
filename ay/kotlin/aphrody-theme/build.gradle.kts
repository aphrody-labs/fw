plugins {
    alias(libs.plugins.android.library)
    alias(libs.plugins.kotlin.compose)
    `maven-publish`
}

val repoRoot = rootProject.projectDir.parentFile
val tokensDir = repoRoot.resolve("brand/tokens")

android {
    namespace = "${project.group}.theme"
    compileSdk { version = release(37) { minorApiLevel = 1 } }
    defaultConfig { minSdk = 23 }
    sourceSets.getByName("main").kotlin.directories.add(tokensDir.resolve("compose").path)
    testOptions.unitTests.all {
        it.systemProperty("aphrody.tokens", tokensDir.resolve("m3-tokens.json").path)
        it.systemProperty("aphrody.theme.package", "${project.group}.theme")
    }
    publishing { singleVariant("release") { withSourcesJar() } }
}

kotlin { compilerOptions { allWarningsAsErrors.set(true) } }

dependencies {
    api(platform(libs.compose.bom))
    api(libs.compose.material3)
    implementation(libs.compose.foundation)

    testImplementation(kotlin("test-junit"))
    testImplementation(libs.junit)
    testImplementation(libs.kotlinx.serialization.json)
}

// GitHub Packages Maven registry of this repository; GITHUB_REPOSITORY, GITHUB_ACTOR and GITHUB_TOKEN come from Actions.
publishing {
    publications {
        register<MavenPublication>("release") {
            artifactId = project.name
            afterEvaluate { from(components["release"]) }
            pom {
                name.set("Aphrody Compose theme")
                description.set("The Aphrody Material 3 Expressive theme for Jetpack Compose, generated from brand/tokens/m3-tokens.json.")
                url.set("https://github.com/${providers.environmentVariable("GITHUB_REPOSITORY").getOrElse("aphrody-labs/ay")}")
            }
        }
    }
    repositories {
        providers.environmentVariable("GITHUB_REPOSITORY").orNull?.let { repository ->
            maven {
                name = "githubPackages"
                url = uri("https://maven.pkg.github.com/$repository")
                credentials {
                    username = providers.environmentVariable("GITHUB_ACTOR").orNull
                    password = providers.environmentVariable("GITHUB_TOKEN").orNull
                }
            }
        }
    }
}
