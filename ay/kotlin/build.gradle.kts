plugins {
    alias(libs.plugins.android.library) apply false
    alias(libs.plugins.kotlin.compose) apply false
}

val repoRoot = rootDir.parentFile

val rootDomain = Regex("""(?m)^root\s*=\s*"([^"]+)"""").find(repoRoot.resolve("ay.toml").readText().substringAfter("[domains]"))
    ?.groupValues?.get(1) ?: error("ay.toml [domains].root is not set")

val workspaceVersion = Regex("""(?m)^version\s*=\s*"([^"]+)"""").find(repoRoot.resolve("Cargo.toml").readText().substringAfter("[workspace.package]"))
    ?.groupValues?.get(1) ?: error("Cargo.toml [workspace.package].version is not set")

allprojects {
    group = rootDomain.split('.').reversed().joinToString(".")
    version = workspaceVersion
}
