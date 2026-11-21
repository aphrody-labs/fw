// @aphrody/linux-types: Linux kernel UAPI for Bun, generated from the kernel itself.
//
// The per-arch modules hold the generated data: import them as `@aphrody/linux-types/x86_64` or
// `@aphrody/linux-types/aarch64`. This module holds the arch-independent runtime (struct codecs,
// syscall and ioctl helpers) and the host arch detection.
export * from "./runtime/index.ts";

export type LinuxArch = "x86_64" | "aarch64";

// The UAPI arch this process runs on, or undefined when it is not Linux on a generated arch.
export function hostArch(): LinuxArch | undefined {
  if (process.platform !== "linux") return undefined;
  if (process.arch === "x64") return "x86_64";
  if (process.arch === "arm64") return "aarch64";
  return undefined;
}
