plugins {
    alias(libs.plugins.kotlin.jvm)
    alias(libs.plugins.kotlin.serialization)
    `maven-publish`
}

kotlin {
    jvmToolchain(25)
    compilerOptions {
        allWarningsAsErrors.set(true)
    }
}

dependencies {
    api(libs.kotlinx.serialization.json)
    api(libs.kotlinx.coroutines.core)

    testImplementation(kotlin("test"))
    testImplementation(libs.junit.jupiter)
    testImplementation(libs.kotlinx.coroutines.test)
    testRuntimeOnly(libs.junit.platform.launcher)
}

tasks.test {
    useJUnitPlatform()
}

// FwProduct.VERSION is product.toml's `version`, written at build time so no source file repeats it.
val generateFwProduct = tasks.register("generateFwProduct") {
    val version = gradle.extra["fw.version"] as String
    val outputDirectory = layout.buildDirectory.dir("generated/fw-product")
    inputs.property("version", version)
    outputs.dir(outputDirectory)
    doLast {
        val directory = outputDirectory.get().dir("com/aphrody/fw/core").asFile
        directory.mkdirs()
        directory.resolve("FwProduct.kt").writeText(
            "package com.aphrody.fw.core\n\n/** Generated from product.toml by core/kotlin/fw-core/build.gradle.kts. */\nobject FwProduct {\n    const val VERSION: String = \"$version\"\n}\n",
        )
    }
}

kotlin.sourceSets.main {
    kotlin.srcDir(generateFwProduct)
}

publishing {
    publications {
        create<MavenPublication>("maven") {
            from(components["java"])
            artifactId = "fw-core"
            pom {
                name.set("Fw Core Kotlin SDK")
                description.set("Core Kotlin SDK for fw and all aphrody-labs repositories (OpenAI, MCP, ACP, A2A, VFS, Brain, Cluster).")
                url.set("https://github.com/aphrody-labs/fw")
            }
        }
    }
}
