# android-lab: Android toolchain, Rust-native Android, fw-kotlin FFI, Compose M3, pixel tester (queued, 1 of 6 units done)

## Goal
Repo `aphrody-labs/android-lab` (clone `$WORKSPACE_ROOT/android-lab`, public, branch main) delivering:
1. `tooling/android/setup.ts` (typed Bun): pinned cmdline-tools, platform-tools, build-tools, NDK LTS, CMake, emulator, Android Studio check, newest platform + system image, AVD with host GPU (RTX 4070), boot + `adb exec-out screencap`; APK tools (apktool, jadx, bundletool, apksigner, Studio analyzer CLI); F-Droid (fdroidserver via uv, local signed repo); Play Publishing API tooling (docs only, no account creation, no upload); Aurora Store from F-Droid on the emulator.
2. Rust-native Android app (cargo-ndk, android-activity, wgpu Vulkan) on the Android 17 emulator rendering an M3 screen, pixels captured; AOSP Rust inventory and build sizing.
3. `fw-kotlin`: Panama FFM over `native/fw-abi` C ABI vs UniFFI, chosen with call-overhead ns and binary size; `crates/fw-kotlin` + Gradle module in `$WORKSPACE_ROOT/fw` (existing: `core/kotlin/fw-rust-ffi`, `native/fw-abi`); tests both directions.
4. `apps/m3-showcase` Compose Multiplatform (Android, Desktop, Wasm), M3 Expressive with fw-m3 roles (`$WORKSPACE_ROOT/fw/crates/fw-m3/tokens/google_design_tokens.toml`), Google Sans Flex, Byron Love sprites (`$WORKSPACE_ROOT/openpets/packages/aphrody/dist/byron-love/spritesheet.webp`), hot reload latencies (Compose Hot Reload, Live Edit, Wasm dev server).
5. `tooling/test/pixels.ts`: Roborazzi/Paparazzi (Android), Compose Desktop screenshot tests, Playwright/agent-browser (web), tolerance 0, diff images, per-platform baselines, one table.
6. Docs + skill `android-lab` registered in `$WORKSPACE_ROOT/fw/plugins/aphrody/sources.toml` (`[[source]]` with `local`, see the `modern-web` entry; format in `scripts/unify.ts`).

## Done (verified on origin)
- `aphrody-labs/android-lab` created; commit `df4e70b9be066a83088852799110e8752999cffe` on origin/main: `tooling/aosp/inventory.ts|json`, `sizing.ts|json`, `docs/aosp.md`. Facts from it: tag `android-17.0.0_r1` (API 37); 1,087 projects; Rust 40,122 files, 16,564,764 lines (exact), 2,342 crate dirs, about 6.28% of estimated code lines; first-party Rust 1,817,982 lines; AOSP requirements 400 GB disk, 64 GB RAM, about 6 h on 6 cores; fr1 12 cores / 45.9 GB RAM (below the 64 GB minimum), `/srv/xbuild` 740 GB free; Cloud Build default pool max `e2-highcpu-32` (32 GB RAM), max timeout 24 h, private pool access PERMISSION_DENIED for the active account.
- Live discovery (this session, PC): Android 17 = API 37.0 (`platforms/android-37.0`, `ro.build.version.sdk_full=37.0`, release 17, REL) and 37.2 minor (`platforms/android-37.2`, REL, ext level 24): stable, not preview. NDK LTS per developer.android.com/ndk/downloads is r30 = `30.0.16248370` (already installed). Latest canary platform `android-canary-20261104`.

## Installed on the PC (pre-existing, found live; nothing installed by this run)
SDK `~/AppData/Local/Android/Sdk` (ANDROID_HOME set): cmdline-tools 23.0.0, platform-tools 37.0.1 (37.0.3 available), build-tools 34.0.0 to 37.0.0, emulator 37.2.13 (37.3.10 available; WHPX accel usable), cmake 4.1.2, NDK 27.0.12077973 / 29.0.14206865 / 30.0.16248370, platforms 35, 36, 37.0, 37.2, canary-20261104, system images: `android-37.0/google_apis/x86_64`, `android-37.2/google_apis_playstore_ps16k/x86_64`, `android-37.2/google_apis_ps16k/x86_64`, `android-canary-20261104/google_apis_ps16k/x86_64`. Android Studio `C:\Program Files\Android\Android Studio` = "Android Studio Rabbit 1 | 2026.2.1", build AI-262.9437.185 (needs JDK 25). android CLI 1.1.16919734. JDK `C:\Program Files\Microsoft\jdk-25.0.5.21-hotspot`, kotlinc 2.4.21, gradle 9.9.1, bun, uv, cargo (`D:\cache\cargo`). GPU RTX 4070, driver 621.14. Disks: C: 278 GB free, D: 50 GB free.
Existing AVDs (other sessions, do not edit): `Aphrody_Play_37`, `ay-x64`, `bun_pixel_api37` (the last has `hw.gpu.mode=host`). Create new ones as `android_lab_*`.

## Remaining
1. `setup.ts` + pins file; `android sdk update` for emulator 37.3.10 and platform-tools 37.0.3; AVDs `android_lab_api37` (google_apis_ps16k, GPU host) and `android_lab_play37` (playstore image), boot ms measured, `adb exec-out screencap -p > shot.png` inspected.
2. APK tools: apktool, jadx, bundletool (GitHub releases, versions read live), analyzer CLI if in the Studio bin; fdroidserver (`uv tool install fdroidserver`), local repo `fdroid init`, sign index, publish a built APK; Play API doc (service account flow) only; Aurora Store APK from F-Droid via `adb install`.
3. Rust app `apps/rust-wgpu` (cargo-ndk + android-activity + wgpu), `cargo ndk -t x86_64 build`, run on emulator, screencap.
4. fw-kotlin: benchmark ns/call and binary size for FFM vs UniFFI, then build `$WORKSPACE_ROOT/fw/crates/fw-kotlin` (heavy compile on fr1 in `/srv/xbuild/<task>/`, never `/`: fr1 `/` is 99% full) and the Gradle module; tests both directions.
5. `apps/m3-showcase` + hot reload numbers; 6. `tooling/test/pixels.ts` + baselines; 7. docs, skill `android-lab`, `sources.toml` entry, `claude plugin update aphrody@fw`.

## Resume
```
cd $WORKSPACE_ROOT/android-lab && git pull origin main
export ANDROID_HOME=/c/Users/aphro/AppData/Local/Android/Sdk
emulator -list-avds ; sdkmanager --list_installed
bun tooling/aosp/inventory.ts   # re-runs the AOSP inventory (cache D:\cache\aosp-inventory)
```
Skills to read first: `$WORKSPACE_ROOT/fw/plugins/aphrody/skills/{android,android-compose,kotlin-compose-m3,kotlin-rust-ffi,wgpu,fleet,disks}\SKILL.md`. Repo rules: main only, commit by path, no AI trailers; fw/aphrody have other sessions' edits, so commit via a temporary index on origin/main.

## Directories created and processes
- `$WORKSPACE_ROOT/android-lab` (git clone, small): keep.
- `D:\cache\aosp-inventory` 4.9 MB (AOSP inventory cache): safe to delete.
- Nothing created on fr1. No emulator running (`adb devices` empty), no downloads in progress, no SDK changes made.
