#![no_std]
#![deny(unsafe_code)]

use ostd::prelude::*;

#[ostd::main]
fn kernel_main() {
    println!("fw-os-boot {} booted", env!("CARGO_PKG_VERSION"));
}
