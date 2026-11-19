plugins {
    alias(libs.plugins.kotlin.jvm) apply false
    alias(libs.plugins.kotlin.serialization) apply false
    alias(libs.plugins.kotlin.compose) apply false
    alias(libs.plugins.compose.multiplatform) apply false
}

val fwGroup = providers.gradleProperty("fw.group").orElse("dev.aphrody.fw").get()
val fwVersion = providers.gradleProperty("fw.version").orElse("4.2.0").get()

subprojects {
    group = fwGroup
    version = fwVersion

    repositories {
        google()
        mavenCentral()
    }
}
