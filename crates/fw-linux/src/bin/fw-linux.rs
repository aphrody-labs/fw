//! `fw-linux`: the Linux kernel as pinned files (run by `fw kernel`). See `fw-linux help`.

use std::process::ExitCode;

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    match fw_linux::cli::run(&arguments) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("fw-linux: {error}");
            ExitCode::FAILURE
        }
    }
}
