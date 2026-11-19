plugins {
    alias(libs.plugins.kotlin.jvm)
    alias(libs.plugins.kotlin.serialization)
    alias(libs.plugins.kotlin.compose)
    alias(libs.plugins.compose.multiplatform)
    `maven-publish`
}

kotlin {
    jvmToolchain(25)
    compilerOptions {
        allWarningsAsErrors.set(true)
    }
}

dependencies {
    api(project(":fw-core"))
    api("org.jetbrains.compose.runtime:runtime:1.12.2")
    api("org.jetbrains.compose.foundation:foundation:1.12.2")
    api("org.jetbrains.compose.material3:material3:1.13.0-rc01")
    api("org.jetbrains.compose.ui:ui:1.12.2")

    testImplementation(kotlin("test"))
    testImplementation(libs.junit.jupiter)
    testRuntimeOnly(libs.junit.platform.launcher)
}

tasks.test {
    useJUnitPlatform()
}

publishing {
    publications {
        create<MavenPublication>("maven") {
            from(components["java"])
            artifactId = "fw-material3"
            pom {
                name.set("Fw Compose Material 3 1.5.0 Expressive Library")
                description.set("Production Compose Material 3 1.5.0 Expressive design system, tokens, components, and M2->M3 migration catalog for fw and all aphrody-labs repositories.")
                url.set("https://github.com/aphrody-labs/fw")
            }
        }
    }
}
