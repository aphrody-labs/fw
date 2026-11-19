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
    api(project(":fw-core"))
    api(libs.kotlinx.serialization.json)

    testImplementation(kotlin("test"))
    testImplementation(libs.junit.jupiter)
    testRuntimeOnly(libs.junit.platform.launcher)
}

tasks.test {
    useJUnitPlatform()
    jvmArgs("--enable-native-access=ALL-UNNAMED")
}

publishing {
    publications {
        create<MavenPublication>("maven") {
            from(components["java"])
            artifactId = "fw-rust-ffi"
            pom {
                name.set("Fw Bidirectional Kotlin <-> Rust FFI")
                description.set("Zero-JNI JDK 25 Panama FFM bridge (Kotlin->Rust downcalls + Rust->Kotlin upcall callbacks) and UniFFI 0.32.2 / JNI 0.22.4 best-practice contracts for fw_abi.")
                url.set("https://github.com/aphrody-labs/fw")
            }
        }
    }
}
