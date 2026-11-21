---
name: android
description: "Android platform work: android CLI, AGP and build upgrades, testing, edge-to-edge, security, R8, ANR, profiling, Play Billing and policy, identity, CameraX, Media3, cars, iOS to Android."
---
# Android platform

Pick the reference for the Android task; Compose UI is in `android-compose`, on-device AI in `android-ai`.

Read the reference that matches the task in full before acting. Paths written inside reference files are relative to this skill's directory (the one holding this SKILL.md). Where a reference names another merged skill, open `references/<name>.md`; its helper files are under `references/<name>/`.

| Reference | Use when |
| --- | --- |
| [references/android-cli.md](references/android-cli.md) | Provides instructions for installing and using the android CLI. The android command-line tool is a critical tool for Android development and helps you create new Android projects |
| [references/agp-9-upgrade.md](references/agp-9-upgrade.md) | Upgrades, or migrates, an Android project to use Android Gradle Plugin (AGP) version 9. Do not Use for migrating Kotlin Multiplatform (KMP) projects. |
| [references/testing-setup.md](references/testing-setup.md) | Analyze and create a testing strategy for Android apps - install testing libraries, set up test infrastructure, create harnesses for unit tests, UI tests, screenshot tests, and end-to-end tests. |
| [references/edge-to-edge.md](references/edge-to-edge.md) | Use to migrate your Jetpack Compose app to add adaptive edge-to-edge support and. Use to fix UI components (like buttons or lists) that are obscured by or overlapping with the navigation bar |
| [references/ios-to-android.md](references/ios-to-android.md) | Use for ANY task that ports or implements Android functionality from\ \ an iOS/Swift codebase \u2014 including single-feature tasks (BLE scanning, location/GPS\ \ tracking, billing, Health Connect |
| [references/restore-credentials.md](references/restore-credentials.md) | Provides knowledge and workflows to implement Android's Restore Credentials feature using the. Use to create, sign in with, and delete restore keys, enabling silent user sign-in on new devices after a |
| [references/verified-email.md](references/verified-email.md) | Provides a complete workflow for implementing verified email retrieval on Android Credential. Use to integrate a secure, OTP-less email verification flow into an Android app. This skill solves the |
| [references/play-billing-library-version-upgrade.md](references/play-billing-library-version-upgrade.md) | Use when upgrading or migrating an Android project from any legacy Google Play Billing Library (PBL) version to the latest stable version of PBL. |
| [references/play-policy-insights.md](references/play-policy-insights.md) | Automated auditor designed to verify Android applications against Google Play Policy domains. It cross-references static code analysis with Play Store declarations to generate deterministic |
| [references/engage-sdk-integration.md](references/engage-sdk-integration.md) | Helps developers integrate, debug, and resolve Play Engage SDK implementation issues. Use when adding Engage SDK support, generating publishing code, mapping data classes to entities |
| [references/android-intent-security.md](references/android-intent-security.md) | Audits, detects, and remediates Android Intent and component security vulnerabilities. Use when auditing component configurations in AndroidManifest.xml (activities, services |
| [references/android-permissions-security.md](references/android-permissions-security.md) | Audits, detects gaps, and remediates Android permissions and IPC component security vulnerabilities. Use when reviewing AndroidManifest.xml, defining custom permissions, securing Services, Receivers |
| [references/r8-analyzer.md](references/r8-analyzer.md) | Analyzes Android build files and R8 keep rules to identify redundancies. Use when developers want to optimize their Android app's size, remove redundant or overly broad keep rules |
| [references/anr-audit-and-fix.md](references/anr-audit-and-fix.md) | Audits Android codebases for ANR risks and diagnoses ANR stack dumps. Use when scanning Android code for ANR risks or triaging ANR stack dumps. Don't use for non-ANR crashes, dropped frames |
| [references/android-profiler.md](references/android-profiler.md) | Records and analyzes Android performance data and diagnoses performance bottlenecks across user. Use when: recording system traces, CPU call stacks, Vulkan GPU/DRM memory, or Java/native heap profiles |
| [references/camerax.md](references/camerax.md) | Provide technical guidance for Android camera development with CameraX. Use when implementing camera features, handling asynchronous recording lifecycles |
| [references/media3-cast-integration.md](references/media3-cast-integration.md) | Implements Google Cast support in Android apps using Jetpack Media3. Handles adding build dependencies. Use when adding Cast functionality or migrating from legacy Cast SDK to Media3 Cast. |
| [references/android-for-cars-parked-apps.md](references/android-for-cars-parked-apps.md) | Adapt existing Android video streaming apps, games. Use when migrating, building, or auditing parked apps for cars to meet Car App Quality guidelines. Don't use |

<!-- merge-skill-families:v1 -->
