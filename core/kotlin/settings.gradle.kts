// Kotlin workspace of fw. Names and versions come from ../../product.toml [kotlin]; nothing is repeated here.
//
// The Aphrody Compose theme (com.aphrody:aphrody-theme) is owned by aphrody-labs/ay and consumed as a published artifact from its
// GitHub Packages Maven registry (read:packages: GITHUB_ACTOR and GITHUB_TOKEN, or the gpr.user and gpr.key Gradle properties).
// With the Gradle property `aphrody.ay.checkout` (a local ay checkout, set in a git-ignored or user-level gradle.properties or with -P)
// that checkout's kotlin/ build is included instead and substitutes the artifact in place: nothing is copied.
val product = file("../../product.toml").readText()

fun product(table: String?, key: String): String {
    val body = if (table == null) product.substringBefore("\n[") else product.substringAfter("\n[$table]\n").substringBefore("\n[")
    return Regex("""(?m)^$key\s*=\s*"([^"]*)"""").find(body)?.groupValues?.get(1)
        ?: error("product.toml ${table?.let { "[$it] " }.orEmpty()}$key is not set")
}

gradle.extra["fw.group"] = product("kotlin", "group")
gradle.extra["fw.version"] = product(null, "version")
gradle.extra["fw.theme"] = "${product("kotlin", "theme_group")}:${product("kotlin", "theme_artifact")}:${product("kotlin", "theme_version")}"

pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.PREFER_PROJECT)
    repositories {
        google()
        mavenCentral()
        maven {
            name = "aphrodyThemeRegistry"
            url = uri(product("kotlin", "theme_registry"))
            credentials {
                username = providers.environmentVariable("GITHUB_ACTOR").orElse(providers.gradleProperty("gpr.user")).orNull
                password = providers.environmentVariable("GITHUB_TOKEN").orElse(providers.gradleProperty("gpr.key")).orNull
            }
            content { includeGroup(product("kotlin", "theme_group")) }
        }
    }
}

providers.gradleProperty("aphrody.ay.checkout").orNull?.let { includeBuild(file(it).resolve("kotlin")) }

rootProject.name = "fw-kotlin"

include(
    ":fw-core",
    ":fw-material3",
    ":fw-rust-ffi",
)
