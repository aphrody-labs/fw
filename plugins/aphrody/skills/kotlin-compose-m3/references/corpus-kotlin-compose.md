# Kotlin and Compose Multiplatform from the corpus

Distilled from the local corpus on `fr1` (`fw-corpus`, read 2026-11-21); paraphrased, each fact cites the stored URL. Re-query before relying on a version number: `fw-corpus search "<words>" --source kotlin|compose-multiplatform`.

## Sources

| id | holds | published at |
| --- | --- | --- |
| `kotlin` | `docs/topics` of `JetBrains/kotlin-web-site` | `https://kotlinlang.org/docs/` |
| `compose-multiplatform` | `JetBrains/compose-multiplatform` docs, `JetBrains/kotlin-multiplatform-dev-docs` `topics/`, `JetBrains/compose-hot-reload` docs | GitHub blob URLs pinned at the crawled commit |

## Compose Hot Reload

- It applies code changes to the running desktop app, unlike Compose previews that render isolated components with test data. Its Gradle plugin needs Kotlin 2.1.20 or newer and a JVM target compatible with Java 21 or earlier; the full experience wants the Kotlin Multiplatform IDE plugin (IntelliJ IDEA 2025.2.2+, Android Studio Otter 2025.2.1+). Source: `https://github.com/JetBrains/kotlin-multiplatform-dev-docs/blob/f55cabfc9116757d5a443ccd2522476cbef4364b/topics/tools/compose-hot-reload.md#add-compose-hot-reload-to-your-project`.
- From Compose Multiplatform 1.10.0 the plugin is bundled and on by default for every project with a desktop target; upgrading is the whole setup. Source: same page, `#to-an-existing-project`.
- Before 1.10.0: configure a desktop target, add `org.jetbrains.compose.hot-reload` to the version catalog, declare it `apply false` in the root `plugins {}` and apply it in the app module, run on JetBrains Runtime (JBR, which supports enhanced class redefinition; automatic provisioning needs the `org.gradle.toolchains.foojay-resolver-convention` plugin in `settings.gradle.kts`). The latest JBR supports only Java 21, so a project that needs Java 22+ fails with a linkage error. Source: same page, "Earlier versions of Compose Multiplatform".
- Known limits are listed in `https://github.com/JetBrains/compose-hot-reload/blob/c1971409aa66b19d11e13db39e276c5fb0acac22/docs/Known_limitations.md`.
