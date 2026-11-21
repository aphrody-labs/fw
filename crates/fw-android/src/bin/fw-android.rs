//! `fw-android`: Android apps without WSA (run by `fw android`). See `fw-android help`.

use std::process::ExitCode;

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    match fw_android::cli::run(&arguments) {
        Ok(code) => ExitCode::from(code),
        Err(message) => {
            eprintln!("fw-android: {message}");
            ExitCode::FAILURE
        }
    }
}
