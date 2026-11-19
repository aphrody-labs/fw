fn main() {
    println!("cargo:rustc-env=ALACRITTY_TERMINAL_VERSION=0.26.0");
    println!("cargo:rustc-env=PORTABLE_PTY_VERSION=0.9.0");
    println!("cargo:rustc-env=RIO_VT_VERSION=0.5.3");
}
