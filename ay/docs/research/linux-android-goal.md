# Goal dig: one optimized Linux for the 7.5 cycle that boots Android 17+ and a Wayland desktop in WSL, QEMU and Docker

Dug 2026-11-17 from local repositories and live remote sources (GitHub API, kernel.org, android.googlesource.com, Docker Hub).
Every row below is a probe in `claims.toml` (`bun tools/claims.ts`) or a command that can be rerun. The goal outranks the
published version numbers: versions only matter where they decide what the kernel must contain.

## What decides the design
1. **Android needs the Rust binder on every kernel from 7.4.** `drivers/android/binder.c` was removed by commit `fd2bc059ed46`
   ("binder: rm -f binder.c", 2026-09-17, Alice Ryhl's series): the commit is an ancestor of `v7.4-rc1`, so 7.4 and the 7.5 cycle have
   no C binder. `drivers/android/Kconfig` on master offers only `ANDROID_BINDER_IPC_RUST` (depends on `MMU`, `NET`, `RUST`, default n)
   and `ANDROID_BINDER_DEVICES` (default `binder,hwbinder,vndbinder`); `drivers/staging` has no android or ashmem entry. A kernel for
   Waydroid, redroid, Cuttlefish or any Android container therefore needs `CONFIG_RUST=y` and the Rust toolchain at build time:
   the minimums read from `Documentation/process/changes.rst` on master are Rust 1.85.0 and bindgen 0.71.1 (both marked optional there,
   which stops being true for this driver). "Full Rust mode" is a requirement of the goal, not a preference.
2. **WSL's own kernel cannot do it.** Microsoft's 6.18 tree (`linux-msft-wsl-6.18.y`) still has the C `binder.c`, but `config-wsl`
   sets `CONFIG_RUSTC_VERSION=0` (Rust off) and enables no `CONFIG_ANDROID_BINDER*` option: Android in WSL needs a custom kernel
   (binder, binderfs, memfd, vsock, plus Rust on 7.4+). `CONFIG_MEMFD_CREATE=y`, `CONFIG_HYPERV_VSOCKETS=y` are already set.
3. **Android 17 userspace exists, Android 18 is a kernel only.** AOSP tag `android-17.0.0_r1` and branch `android-latest-release`;
   the common kernel has `android18-7.3`, `android17-6.18-p3s`, `android-mainline`, `android-mainline-desktop`,
   `android-mainline-rust`. No `android-18*` manifest ref. redroid's images stop at 16.0.0; Waydroid and Cuttlefish are active.
4. **The desktop half already exists in the estate**: aphrody `crates/os_desktop` (COSMIC Epoch shell on GPUI, binary `aphrody-os`,
   `gpui_linux`, `gpui_app_shell`), forks of cosmic-*, smithay, niri, Hyprland, gamescope, aquamarine, greenfield, wayland-c in
   aphrody-labs, legacy `src/wayland/*` and `src/posix/wslg`. Whether `aphrody-os` is a compositor or a client shell on one is not
   established.

## Prior art found
- `Veritas-Quaesitor/redroid-wsl2-android-lab` (pushed 2026-10-31): redroid in Docker on Windows 11 WSL2 with Play Store; a recipe on
  the stock kernel generation, not a mainline-ready one.
- GitHub repository search returned nothing for Waydroid on a custom WSL2 kernel, Android on a rebased WSL kernel, or Cuttlefish
  with binderfs in Docker: the combination in the goal has no published recipe to reuse.

## Still to dig (not done)
Android 17 kernel requirements (`android17-6.18-*` defconfigs and fragments), `android-mainline-desktop` and `-rust` deltas,
Waydroid's kernel requirement list and image support for Android 17, lore.kernel.org threads on Rust binder and its first users
outside Android (container hosts), Cuttlefish's kernel needs, linux-next's Rust-gated drivers, and whether our `rust-mono`
toolchain (rustc 1.85+) builds the tree with `make LLVM=1 rustavailable` on the build host.

## Sources (retrieved 2026-11-17)
`api.github.com/repos/torvalds/linux` (commits, compare, contents of `drivers/android`, `Documentation/process/changes.rst`),
`api.github.com/repos/microsoft/WSL2-Linux-Kernel` (contents on `linux-msft-wsl-6.18.y`), `git ls-remote` of
`android.googlesource.com/platform/manifest` and `/kernel/common`, Docker Hub tags of `redroid/redroid`, GitHub repository search.
