import { expect, test } from "bun:test";
import { transform, type Pins } from "./linux-aphrody.ts";

const pins: Pins = { arch_commit: "abc123", tag: "v7.5-rc1", fragments_repo: "o/r", fragments_commit: "def456", fragments_dir: "d", fragments: ["rust.config", "wsl.config"], sha256: { "config.x86_64": "c".repeat(64), "rust.config": "r".repeat(64), "wsl.config": "w".repeat(64) } };

const arch = `# Maintainer: Someone <a@example.test>

pkgbase=linux
pkgver=7.3.4.arch1
pkgrel=2
makedepends=(
  bc
  rust
  # htmldocs
  graphviz
)
source=(x)
export KBUILD_BUILD_HOST=archlinux
prepare() {
  cd $_srcname

  echo "Setting version..."
  echo "-$pkgrel" > localversion.10-pkgrel

  local src
  for src in "\${source[@]}"; do
    patch -Np1 < "../$src"
  done

  echo "Setting config..."
  cp ../config.$CARCH .config
  make olddefconfig

  make -s kernelrelease > version
}

build() {
  cd $_srcname
  make all
  make -C tools/bpf/bpftool vmlinux.h feature-clang-bpf-co-re=1
}
`;

test("the Aphrody PKGBUILD keeps Arch's body and changes only the documented parts", () => {
  const out = transform(arch, pins);
  expect(out).toContain("pkgbase=linux-aphrody");
  expect(out).toContain("pkgver=7.5rc1");
  expect(out).toContain("#tag=$_srctag?signed");
  expect(out).toContain("  clang\n  llvm\n  lld");
  expect(out).toContain("  graphviz");
  expect(out).not.toContain("# htmldocs");
  expect(out).toContain("merge_config.sh -m .config ../rust.config ../wsl.config");
  expect(out).toContain("grep -q '^CONFIG_RUST=y'");
  expect(out).toContain("make LLVM=1 all");
  expect(out).not.toContain("Applying patch");
  expect(out).toContain('echo "-$pkgrel" > localversion.10-pkgrel');
  expect(out).toContain(`sha256sums_x86_64=('${"c".repeat(64)}')`);
  expect(out).toContain("https://raw.githubusercontent.com/o/r/def456/d/rust.config");
});

test("a changed upstream shape is an error, never a silently wrong package", () => {
  expect(() => transform(arch.replace("  make all\n", "  make everything\n"), pins)).toThrow("anchor not found");
  expect(() => transform(arch.replace("  local src\n", ""), pins)).toThrow("prepare() anchors");
  expect(() => transform(arch.replace("makedepends=(", "builddeps=("), pins)).toThrow("makedepends");
});
