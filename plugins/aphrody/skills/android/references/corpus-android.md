# Android docs from the corpus

Distilled from the local corpus on `fr1` (`fw-corpus`, read 2026-11-21); paraphrased, each fact cites the stored URL. Re-query before relying on a detail: `fw-corpus search "<words>" --source android-dev|android-source|aosp-docs|aosp-repos`.

## Sources

| id | holds | notes |
| --- | --- | --- |
| `android-dev` | developer.android.com guides as markdown (`<page>.md.txt`), API reference excluded | sitemap-driven; `lastmod` decides what is refetched |
| `android-source` | source.android.com | sitemap-driven |
| `aosp-docs` | `platform/developers/docs`, `platform/build/soong` docs, `platform/art` docs and similar trees on android.googlesource.com | pinned to the HEAD commit read |
| `aosp-repos` | repository inventory of android.googlesource.com with README text | one document per repository |

Stored headings of `android-dev` pages come from the URL (the markdown carries no title), so search by path words such as `about versions 17 behavior changes`.

## Android 17 (API level 37) behavior changes for all apps

Source: `https://developer.android.com/about/versions/17/behavior-changes-all`. Apps that target 37 have a second list at `https://developer.android.com/about/versions/17/behavior-changes-17`.

- App memory limits scale with the device's total RAM and target leaks and outliers. An affected session exits with `ApplicationExitInfo` reason `REASON_OTHER` and a description containing `MemoryLimiter:AnonSwap`; `TRIGGER_TYPE_ANOMALY` trigger-based profiling collects a heap dump when the limit is hit. On a device that imposes limits, `adb shell am memory-limiter ignore|manual|status` adjusts or disables them. Anchor: `#app-memory-limits`.
- Cross-profile loopback traffic is blocked by default; loopback inside one profile is unchanged, whatever the target SDK. Anchor: `#block-cross-profile-loopback-traffic`.
- After a configuration change the app does not handle (rotation), the previous IME visibility is no longer restored; ask for the keyboard explicitly with `android:windowSoftInputMode="stateAlwaysVisible"` or in `onCreate()` / `onConfigurationChanged()`. Anchor: `#restoring-default-ime-visibility-after-rotation`.
- Other sections on the same page: SMS OTP protection, `usesClearTraffic` deprecation plan, implicit URI grants, per-app keystore limits, relative touchpad events during pointer capture, background audio hardening, Bluetooth bond re-pairing.
