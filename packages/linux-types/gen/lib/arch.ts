// Supported architectures: the name used by this package, the kernel ARCH= value for
// `make headers_install`, and the clang target triple used to compile the headers.
export interface Arch {
  name: string;
  kernelArch: string;
  triple: string;
  pointerBits: 64;
}

export const ARCHES: Record<string, Arch> = {
  x86_64: { name: "x86_64", kernelArch: "x86", triple: "x86_64-linux-gnu", pointerBits: 64 },
  aarch64: { name: "aarch64", kernelArch: "arm64", triple: "aarch64-linux-gnu", pointerBits: 64 },
};

export function archOf(name: string): Arch {
  const arch = ARCHES[name];
  if (!arch) throw new Error(`unknown arch ${name}; known: ${Object.keys(ARCHES).join(", ")}`);
  return arch;
}
