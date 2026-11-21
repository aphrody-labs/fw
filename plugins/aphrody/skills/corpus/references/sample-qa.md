# Five sample questions answered from the corpus

Each answer is paraphrased from the stored document and carries the command that found it, the stored URL and the fetch time (UTC, 2026-11-21).

## 1. Which button configurations does Material 3 Expressive add?

`fw-corpus search "buttons color elevated filled tonal outlined text" --source m3` then `fw-corpus read "components/buttons/specs" --source m3`.

Expressive adds the toggle (selection) variant, the sizes XS, M, L and XL next to the default small, and the square shape next to the default round one. The five colour styles are elevated, filled, tonal, outlined and text, with toggle buttons excluding text. The small button's 24dp padding is not recommended in Expressive; use 16dp. Container and text of a custom colour role must keep a 3:1 contrast ratio.
Citation: `https://m3.material.io/components/buttons/specs#color`, fetched 2026-11-21T09:58:25Z.

## 2. What does Android 17 change about app memory?

`fw-corpus search "Android 17 behavior changes all apps" --source android-dev`.

Apps get memory limits scaled to the device's RAM, aimed at leaks and outliers. An app session that hit the limit exits with `ApplicationExitInfo` reason `REASON_OTHER` and a description containing `MemoryLimiter:AnonSwap`; trigger-based profiling with `TRIGGER_TYPE_ANOMALY` captures a heap dump at that moment; `adb shell am memory-limiter ignore|manual|status` adjusts the limits on a device that imposes them. The change applies to all apps regardless of target SDK.
Citation: `https://developer.android.com/about/versions/17/behavior-changes-all#app-memory-limits`, fetched 2026-11-21T09:23:07Z.

## 3. How do I avoid locks in Chromium code?

`fw-corpus search "threading and tasks PostTask sequences" --source chromium-docs`.

Chromium discourages locks: tasks posted to one sequence run in order, so a class always accessed from the same sequence is thread-safe without a lock even though successive tasks may land on different worker threads. Create a sequence with `base::ThreadPool::CreateSequencedTaskRunner()`.
Citation: `https://chromium.googlesource.com/chromium/src/+/HEAD/docs/threading_and_tasks.md#using-sequences-instead-of-locks`, pinned commit `59fa9c276a82c6b35a37d20ee08637a8b4ad27c0`, fetched 2026-11-21T09:11:25Z.

## 4. What does Cargo's feature resolver 2 change, and which resolver does edition 2024 use?

`fw-corpus search "feature resolver" --source cargo` then `fw-corpus read "reference/resolver.html" --source cargo`.

Resolver 2 no longer unifies features across target-specific dependencies of a target that is not being built, across build-dependencies or proc-macros and the same normal dependency, or from dev-dependencies unless they are being built. Edition 2021 defaults to resolver 2 (Rust 1.51+); edition 2024 defaults to resolver 3 (Rust 1.84+), which only switches `resolver.incompatible-rust-versions` from `allow` to `fallback`. The version is read from the top-level package or the `[workspace]` table.
Citation: `https://doc.rust-lang.org/cargo/reference/resolver.html#feature-resolver-version-2` and `#resolver-versions`, pinned `https://github.com/rust-lang/cargo/blob/ae8f06eb41b8c4a534d0f280f1b9d287f45ead73/doc/book/src/reference/resolver.md`, fetched 2026-11-21T08:58:29Z.

## 5. How do I set up Compose Hot Reload?

`fw-corpus search "Add Compose Hot Reload to your project" --source compose-multiplatform`.

With Compose Multiplatform 1.10.0 or newer and a desktop target there is nothing to add: the plugin is bundled and on by default. Older versions need a desktop target, the `org.jetbrains.compose.hot-reload` plugin (version catalog entry, `apply false` in the root build, applied in the app module) and a JetBrains Runtime, which Gradle can provision through the foojay resolver plugin; the JBR supports only Java 21. The plugin needs Kotlin 2.1.20 or newer.
Citation: `https://github.com/JetBrains/kotlin-multiplatform-dev-docs/blob/f55cabfc9116757d5a443ccd2522476cbef4364b/topics/tools/compose-hot-reload.md#to-an-existing-project`, fetched 2026-11-21T09:10:09Z.
