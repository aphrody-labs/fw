// The Kotlin workspace of ay: Android libraries published to the GitHub Packages Maven registry of this repository.
// Names come from the repository manifests: the group is the reversed root domain of ay.toml [domains], the version is
// the workspace version of Cargo.toml.
pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositories {
        google()
        mavenCentral()
    }
}

rootProject.name = "ay-kotlin"
include(":aphrody-theme")
