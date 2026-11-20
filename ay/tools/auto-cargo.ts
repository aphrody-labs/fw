#!/usr/bin/env bun
// Runs cargo for the Auto units on the nightly toolchain they need (`-Zthreads`, shared generics off on Windows).
//   bun tools/auto-cargo.ts build --release -p aphrody-auto --features runtime --bin auto
export const cargoEnv = {
  ...Bun.env,
  RUSTC_WRAPPER: "",
  RUSTUP_TOOLCHAIN: "nightly",
  CARGO_TERM_COLOR: "never",
  ...(process.platform === "win32"
    ? { CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS: "--cfg reqwest_unstable -C target-cpu=native -Zthreads=8 -Zshare-generics=off" }
    : {}),
};

if (import.meta.main) {
  const proc = Bun.spawn(["cargo", ...Bun.argv.slice(2)], {
    stdin: "inherit",
    stdout: "inherit",
    stderr: "inherit",
    env: cargoEnv,
  });
  process.exitCode = await proc.exited;
}
